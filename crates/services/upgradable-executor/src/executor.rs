use crate::{
    config::Config,
    instance::Instance,
    COMPILED_UNDERLYING_EXECUTOR,
    DEFAULT_ENGINE,
};
#[cfg(any(test, feature = "test-helpers"))]
use fuel_core_storage::transactional::Changes;
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
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
    },
};
use fuel_core_wasm_executor::{
    fuel_core_executor::{
        executor::{
            ExecutionBlockWithSource,
            ExecutionOptions,
            OnceTransactionsSource,
        },
        ports::{
            RelayerPort,
            TransactionsSource,
        },
    },
    utils::ReturnType,
};
use std::sync::Arc;
use wasmtime::{
    Engine,
    Module,
};

pub struct Executor<S, R> {
    pub storage_view_provider: S,
    pub relayer_view_provider: R,
    pub config: Arc<Config>,
    engine: Engine,
    module: Module,
}

impl<S, R> Executor<S, R> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Arc<Config>,
    ) -> Self {
        let engine = DEFAULT_ENGINE.clone();
        let module = COMPILED_UNDERLYING_EXECUTOR.clone();
        Self {
            storage_view_provider,
            relayer_view_provider,
            config,
            engine,
            module,
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
    pub fn execute_without_commit(
        &self,
        block: fuel_core_types::services::executor::ExecutionBlock,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        self.execute_without_commit_with_coinbase(block, Default::default())
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn execute_without_commit_with_coinbase(
        &self,
        block: fuel_core_types::services::executor::ExecutionBlock,
        coinbase_recipient: fuel_core_types::fuel_types::ContractId,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        let component = match block {
            ExecutionTypes::DryRun(_) => {
                panic!("It is not possible to commit the dry run result");
            }
            ExecutionTypes::Production(block) => ExecutionTypes::Production(Components {
                header_to_produce: block.header,
                transactions_source: OnceTransactionsSource::new(block.transactions),
                coinbase_recipient,
                gas_limit: u64::MAX,
            }),
            ExecutionTypes::Validation(block) => ExecutionTypes::Validation(block),
        };

        self.execute_inner(component, self.config.as_ref().into())
    }
}

impl<S, R> Executor<S, R>
where
    S: AtomicView,
    S::View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView<Height = DaBlockHeight>,
    R::View: RelayerPort + Send + Sync + 'static,
{
    pub fn execute_without_commit_with_source<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
        option: ExecutionOptions,
    ) -> ReturnType
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        self.execute_inner(block, option)
    }

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
            consensus_params: self.config.consensus_parameters.clone(),
        };

        let component = Components {
            header_to_produce: component.header_to_produce,
            transactions_source: OnceTransactionsSource::new(
                component.transactions_source,
            ),
            coinbase_recipient: Default::default(),
            gas_limit: component.gas_limit,
        };

        let ExecutionResult {
            skipped_transactions,
            tx_status,
            ..
        } = self
            .execute_without_commit_with_source(
                ExecutionTypes::DryRun(component),
                options,
            )?
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
    ) -> ReturnType
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let mut source = None;
        let block = block.map_p(|component| {
            let Components {
                header_to_produce,
                transactions_source,
                coinbase_recipient,
                gas_limit,
            } = component;

            source = Some(transactions_source);

            Components {
                header_to_produce,
                transactions_source: (),
                coinbase_recipient,
                gas_limit,
            }
        });

        let storage = self.storage_view_provider.latest_view();
        let relayer = self.relayer_view_provider.latest_view();

        let instance = Instance::new(&self.engine)
            .add_input_data(block, options)?
            .add_source(source)?
            .add_storage(storage)?
            .add_relayer(relayer)?;

        instance.run(&self.module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        structured_storage::test::InMemoryStorage,
        Result as StorageResult,
    };
    use fuel_core_types::{
        blockchain::block::Block,
        fuel_types::BlockHeight,
        services::{
            executor::{
                Error as ExecutorError,
                ExecutionTypes,
            },
            relayer::Event,
        },
    };

    struct Storage;

    impl AtomicView for Storage {
        type View = InMemoryStorage<Column>;
        type Height = BlockHeight;

        fn latest_height(&self) -> Option<Self::Height> {
            Some(0u32.into())
        }

        fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
            Ok(self.latest_view())
        }

        fn latest_view(&self) -> Self::View {
            InMemoryStorage::<Column>::default()
        }
    }

    struct DisabledRelayer;

    impl AtomicView for DisabledRelayer {
        type View = DisabledRelayer;
        type Height = DaBlockHeight;

        fn latest_height(&self) -> Option<Self::Height> {
            unimplemented!()
        }

        fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
            unimplemented!()
        }

        fn latest_view(&self) -> Self::View {
            DisabledRelayer
        }
    }

    impl RelayerPort for DisabledRelayer {
        fn enabled(&self) -> bool {
            false
        }

        fn get_events(&self, _: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
            unimplemented!()
        }
    }

    #[test]
    fn test_executor() {
        let executor = Executor::new(Storage, DisabledRelayer, Default::default());
        let result =
            executor.execute_without_commit(ExecutionTypes::Validation(Block::default()));
        assert_eq!(
            result.expect_err("Should be error"),
            ExecutorError::MintMissing
        );
    }
}
