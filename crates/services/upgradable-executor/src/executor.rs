use crate::config::Config;
use fuel_core_executor::{
    executor::{
        ExecutionBlockWithSource,
        ExecutionOptions,
        OnceTransactionsSource,
    },
    ports::{
        RelayerPort,
        TransactionsSource,
    },
};
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Changes,
        Modifiable,
    },
};
#[cfg(any(test, feature = "test-helpers"))]
use fuel_core_types::services::executor::UncommittedResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::{
        block_producer::Components,
        executor::{
            ExecutionResult,
            ExecutionTypes,
            Result as ExecutorResult,
            TransactionExecutionStatus,
        },
        Uncommitted,
    },
};
use std::sync::Arc;

/// The upgradable executor supports the WASM version of the state transition function.
/// If the block has a version the same as a native executor, we will use it.
/// If not, the WASM version of the state transition function will be used
/// (if the database has a corresponding bytecode).
pub struct Executor<S, R> {
    pub storage_view_provider: S,
    pub relayer_view_provider: R,
    pub config: Arc<Config>,
    #[cfg(feature = "wasm-executor")]
    engine: wasmtime::Engine,
    #[cfg(feature = "wasm-executor")]
    module: wasmtime::Module,
}

#[cfg(feature = "wasm-executor")]
mod private {
    use std::sync::OnceLock;
    use wasmtime::{
        Engine,
        Module,
    };

    /// The default engine for the WASM executor. It is used to compile the WASM bytecode.
    pub(crate) static DEFAULT_ENGINE: OnceLock<Engine> = OnceLock::new();

    /// The default module compiles the WASM bytecode of the native executor.
    /// It is used to create the WASM instance of the executor.
    pub(crate) static COMPILED_UNDERLYING_EXECUTOR: OnceLock<Module> = OnceLock::new();
}

impl<S, R> Executor<S, R> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        #[cfg(feature = "wasm-executor")]
        let engine = private::DEFAULT_ENGINE.get_or_init(wasmtime::Engine::default);
        #[cfg(feature = "wasm-executor")]
        let module = private::COMPILED_UNDERLYING_EXECUTOR.get_or_init(|| {
            wasmtime::Module::new(engine, crate::WASM_BYTECODE)
                .expect("Failed to compile the WASM bytecode")
        });

        Self {
            storage_view_provider,
            relayer_view_provider,
            config: Arc::new(config),
            #[cfg(feature = "wasm-executor")]
            engine: engine.clone(),
            #[cfg(feature = "wasm-executor")]
            module: module.clone(),
        }
    }
}

impl<D, R> Executor<D, R>
where
    R: AtomicView<Height = DaBlockHeight>,
    R::View: RelayerPort + Send + Sync + 'static,
    D: AtomicView<Height = BlockHeight> + Modifiable,
    D::View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
{
    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn execute_and_commit(
        &mut self,
        block: fuel_core_types::services::executor::ExecutionBlock,
    ) -> fuel_core_types::services::executor::Result<ExecutionResult> {
        let (result, changes) = self.execute_without_commit(block)?.into();

        self.storage_view_provider.commit_changes(changes)?;
        Ok(result)
    }
}

impl<S, R> Executor<S, R>
where
    S: AtomicView<Height = BlockHeight>,
    S::View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView<Height = DaBlockHeight>,
    R::View: RelayerPort + Send + Sync + 'static,
{
    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and returns the result of the execution with storage changes.
    pub fn execute_without_commit(
        &self,
        block: fuel_core_types::services::executor::ExecutionBlock,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        self.execute_without_commit_with_coinbase(block, Default::default(), 0)
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// The analog of the [`Self::execute_without_commit`] method,
    /// but with the ability to specify the coinbase recipient and the gas price.
    pub fn execute_without_commit_with_coinbase(
        &self,
        block: fuel_core_types::services::executor::ExecutionBlock,
        coinbase_recipient: fuel_core_types::fuel_types::ContractId,
        gas_price: u64,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        let component = match block {
            ExecutionTypes::DryRun(_) => {
                panic!("It is not possible to commit the dry run result");
            }
            ExecutionTypes::Production(block) => ExecutionTypes::Production(Components {
                header_to_produce: block.header,
                transactions_source: OnceTransactionsSource::new(block.transactions),
                coinbase_recipient,
                gas_price,
            }),
            ExecutionTypes::Validation(block) => ExecutionTypes::Validation(block),
        };

        let option = self.config.as_ref().into();
        self.execute_inner(component, option)
    }
}

impl<S, R> Executor<S, R>
where
    S: AtomicView,
    S::View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView<Height = DaBlockHeight>,
    R::View: RelayerPort + Send + Sync + 'static,
{
    /// Executes the block and returns the result of the execution without committing the changes.
    pub fn execute_without_commit_with_source<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let options = self.config.as_ref().into();
        self.execute_inner(block, options)
    }

    /// Executes the block and returns the result of the execution without committing
    /// the changes in the dry run mode.
    pub fn dry_run(
        &self,
        component: Components<Vec<Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        // fallback to service config value if no utxo_validation override is provided
        let utxo_validation =
            utxo_validation.unwrap_or(self.config.utxo_validation_default);

        let options = ExecutionOptions {
            utxo_validation,
            backtrace: self.config.backtrace,
        };

        let component = Components {
            header_to_produce: component.header_to_produce,
            transactions_source: OnceTransactionsSource::new(
                component.transactions_source,
            ),
            coinbase_recipient: Default::default(),
            gas_price: component.gas_price,
        };

        let ExecutionResult {
            skipped_transactions,
            tx_status,
            ..
        } = self
            .execute_inner(ExecutionTypes::DryRun(component), options)?
            .into_result();

        // If one of the transactions fails, return an error.
        if let Some((_, err)) = skipped_transactions.into_iter().next() {
            return Err(err)
        }

        Ok(tx_status)
    }

    fn execute_inner<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        #[cfg(feature = "wasm-executor")]
        return self.wasm_execute_inner(block, options);

        #[cfg(not(feature = "wasm-executor"))]
        return self.native_execute_inner(block, options);
    }

    #[cfg(feature = "wasm-executor")]
    fn wasm_execute_inner<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let mut source = None;
        let block = block.map_p(|component| {
            let Components {
                header_to_produce,
                transactions_source,
                coinbase_recipient,
                gas_price,
            } = component;

            source = Some(transactions_source);

            Components {
                header_to_produce,
                transactions_source: (),
                coinbase_recipient,
                gas_price,
            }
        });

        let storage = self.storage_view_provider.latest_view();
        let relayer = self.relayer_view_provider.latest_view();

        let instance = crate::instance::Instance::new(&self.engine)
            .add_source(source)?
            .add_storage(storage)?
            .add_relayer(relayer)?
            .add_input_data(block, options)?;

        instance.run(&self.module)
    }

    #[cfg(not(feature = "wasm-executor"))]
    fn native_execute_inner<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let storage = self.storage_view_provider.latest_view();
        let relayer = self.relayer_view_provider.latest_view();

        let instance = fuel_core_executor::executor::ExecutionInstance {
            relayer,
            database: storage,
            options,
        };
        instance.execute_without_commit(block)
    }
}
