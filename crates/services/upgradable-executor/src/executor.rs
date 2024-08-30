use crate::config::Config;
#[cfg(feature = "wasm-executor")]
use crate::error::UpgradableError;

use fuel_core_executor::{
    executor::{
        ExecutionInstance,
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
        HistoricalView,
        Modifiable,
    },
};
#[cfg(feature = "wasm-executor")]
use fuel_core_types::fuel_types::Bytes32;
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::{
            StateTransitionBytecodeVersion,
            LATEST_STATE_TRANSITION_VERSION,
        },
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            ExecutionResult,
            Result as ExecutorResult,
            TransactionExecutionStatus,
            ValidationResult,
        },
        Uncommitted,
    },
};
use std::sync::Arc;

#[cfg(feature = "wasm-executor")]
use fuel_core_storage::{
    not_found,
    structured_storage::StructuredStorage,
    tables::{
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
    StorageAsRef,
};
#[cfg(any(test, feature = "test-helpers"))]
use fuel_core_types::blockchain::block::PartialFuelBlock;
#[cfg(any(test, feature = "test-helpers"))]
use fuel_core_types::services::executor::UncommittedResult;

#[cfg(feature = "wasm-executor")]
enum ExecutionStrategy {
    /// The native executor used when the version matches.
    Native,
    /// The WASM executor used even when the version matches.
    Wasm {
        /// The compiled WASM module of the native executor bytecode.
        module: wasmtime::Module,
    },
}

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
    execution_strategy: ExecutionStrategy,
    #[cfg(feature = "wasm-executor")]
    cached_modules: parking_lot::Mutex<
        std::collections::HashMap<
            fuel_core_types::blockchain::header::StateTransitionBytecodeVersion,
            wasmtime::Module,
        >,
    >,
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

#[cfg(feature = "wasm-executor")]
/// The environment variable that forces the executor to use the WASM
/// version of the state transition function.
pub const FUEL_ALWAYS_USE_WASM: &str = "FUEL_ALWAYS_USE_WASM";

impl<S, R> Executor<S, R> {
    /// The current version of the native executor is used to determine whether
    /// we need to use a native executor or WASM. If the version is the same as
    /// on the block, native execution is used. If the version is not the same
    /// as in the block, then the WASM executor is used.
    pub const VERSION: u32 = LATEST_STATE_TRANSITION_VERSION;

    /// This constant is used along with the `version_check` test.
    /// To avoid automatic bumping during release, the constant uses `-` instead of `.`.
    /// Each release should have its own new version of the executor.
    /// The version of the executor should grow without gaps.
    /// If publishing the release fails or the release is invalid, and
    /// we don't plan to upgrade the network to use this release,
    /// we still need to increase the version. The network can
    /// easily skip releases by upgrading to the old state transition function.
    pub const CRATE_VERSIONS: &'static [(
        &'static str,
        StateTransitionBytecodeVersion,
    )] = &[
        ("0-26-0", StateTransitionBytecodeVersion::MIN),
        ("0-27-0", 1),
        ("0-28-0", 2),
        ("0-29-0", 3),
        ("0-30-0", 4),
        ("0-31-0", 5),
        ("0-32-0", 6),
        ("0-32-1", 7),
        ("0-33-0", 8),
        ("0-34-0", 9),
        ("0-35-0", LATEST_STATE_TRANSITION_VERSION),
    ];

    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        #[cfg(feature = "wasm-executor")]
        {
            if std::env::var_os(FUEL_ALWAYS_USE_WASM).is_some() {
                Self::wasm(storage_view_provider, relayer_view_provider, config)
            } else {
                Self::native(storage_view_provider, relayer_view_provider, config)
            }
        }
        #[cfg(not(feature = "wasm-executor"))]
        {
            Self::native(storage_view_provider, relayer_view_provider, config)
        }
    }

    pub fn native_executor_version(&self) -> StateTransitionBytecodeVersion {
        self.config.native_executor_version.unwrap_or(Self::VERSION)
    }

    pub fn native(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        Self {
            storage_view_provider,
            relayer_view_provider,
            config: Arc::new(config),
            #[cfg(feature = "wasm-executor")]
            engine: private::DEFAULT_ENGINE
                .get_or_init(wasmtime::Engine::default)
                .clone(),
            #[cfg(feature = "wasm-executor")]
            execution_strategy: ExecutionStrategy::Native,
            #[cfg(feature = "wasm-executor")]
            cached_modules: Default::default(),
        }
    }

    #[cfg(feature = "wasm-executor")]
    pub fn wasm(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        let engine = private::DEFAULT_ENGINE.get_or_init(wasmtime::Engine::default);
        let module = private::COMPILED_UNDERLYING_EXECUTOR.get_or_init(|| {
            wasmtime::Module::new(engine, crate::WASM_BYTECODE)
                .expect("Failed to validate the WASM bytecode")
        });

        Self {
            storage_view_provider,
            relayer_view_provider,
            config: Arc::new(config),
            engine: engine.clone(),
            execution_strategy: ExecutionStrategy::Wasm {
                module: module.clone(),
            },
            cached_modules: Default::default(),
        }
    }
}

impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight> + Modifiable,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn produce_and_commit(
        &mut self,
        block: PartialFuelBlock,
    ) -> fuel_core_types::services::executor::Result<ExecutionResult> {
        let (result, changes) = self.produce_without_commit(block)?.into();

        self.storage_view_provider.commit_changes(changes)?;
        Ok(result)
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn validate_and_commit(
        &mut self,
        block: &Block,
    ) -> fuel_core_types::services::executor::Result<ValidationResult> {
        let (result, changes) = self.validate(block)?.into();
        self.storage_view_provider.commit_changes(changes)?;
        Ok(result)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight>,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    /// Executes the block and returns the result of the execution with storage changes.
    pub fn produce_without_commit(
        &self,
        block: PartialFuelBlock,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        self.produce_without_commit_with_coinbase(block, Default::default(), 0)
    }

    /// The analog of the [`Self::produce_without_commit`] method,
    /// but with the ability to specify the coinbase recipient and the gas price.
    pub fn produce_without_commit_with_coinbase(
        &self,
        block: PartialFuelBlock,
        coinbase_recipient: fuel_core_types::fuel_types::ContractId,
        gas_price: u64,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        let component = Components {
            header_to_produce: block.header,
            transactions_source: OnceTransactionsSource::new(block.transactions),
            coinbase_recipient,
            gas_price,
        };

        let options = self.config.as_ref().into();
        self.produce_inner(component, options, false)
    }

    /// Executes a dry-run of the block and returns the result of the execution without committing the changes.
    pub fn dry_run_without_commit_with_source<TxSource>(
        &self,
        block: Components<TxSource>,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let options = self.config.as_ref().into();
        self.produce_inner(block, options, true)
    }
}

impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight>,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    /// Produces the block and returns the result of the execution without committing the changes.
    pub fn produce_without_commit_with_source<TxSource>(
        &self,
        components: Components<TxSource>,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let options = self.config.as_ref().into();
        self.produce_inner(components, options, false)
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
            extra_tx_checks: utxo_validation,
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
        } = self.produce_inner(component, options, true)?.into_result();

        // If one of the transactions fails, return an error.
        if let Some((_, err)) = skipped_transactions.into_iter().next() {
            return Err(err)
        }

        Ok(tx_status)
    }

    pub fn validate(
        &self,
        block: &Block,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        let options = self.config.as_ref().into();
        self.validate_inner(block, options)
    }

    #[cfg(feature = "wasm-executor")]
    fn produce_inner<TxSource>(
        &self,
        block: Components<TxSource>,
        options: ExecutionOptions,
        dry_run: bool,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let block_version = block.header_to_produce.state_transition_bytecode_version;
        let native_executor_version = self.native_executor_version();
        if block_version == native_executor_version {
            match &self.execution_strategy {
                ExecutionStrategy::Native => {
                    self.native_produce_inner(block, options, dry_run)
                }
                ExecutionStrategy::Wasm { module } => {
                    self.wasm_produce_inner(module, block, options, dry_run)
                }
            }
        } else {
            let module = self.get_module(block_version)?;
            self.wasm_produce_inner(&module, block, options, dry_run)
        }
    }

    #[cfg(not(feature = "wasm-executor"))]
    fn produce_inner<TxSource>(
        &self,
        block: Components<TxSource>,
        options: ExecutionOptions,
        dry_run: bool,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let block_version = block.header_to_produce.state_transition_bytecode_version;
        let native_executor_version = self.native_executor_version();
        if block_version == native_executor_version {
            self.native_produce_inner(block, options, dry_run)
        } else {
            Err(ExecutorError::Other(format!(
                "Not supported version `{block_version}`. Expected version is `{}`",
                Self::VERSION
            )))
        }
    }

    #[cfg(feature = "wasm-executor")]
    fn validate_inner(
        &self,
        block: &Block,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        let block_version = block.header().state_transition_bytecode_version;
        let native_executor_version = self.native_executor_version();
        if block_version == native_executor_version {
            match &self.execution_strategy {
                ExecutionStrategy::Native => self.native_validate_inner(block, options),
                ExecutionStrategy::Wasm { module } => {
                    self.wasm_validate_inner(module, block, options)
                }
            }
        } else {
            let module = self.get_module(block_version)?;
            self.wasm_validate_inner(&module, block, self.config.as_ref().into())
        }
    }

    #[cfg(feature = "wasm-executor")]
    fn trace_block_version_warning(&self, block_version: StateTransitionBytecodeVersion) {
        tracing::warn!(
            "The block version({}) is different from the native executor version({}). \
                The WASM executor will be used.",
            block_version,
            self.native_executor_version()
        );
    }

    #[cfg(not(feature = "wasm-executor"))]
    fn validate_inner(
        &self,
        block: &Block,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        let block_version = block.header().state_transition_bytecode_version;
        let native_executor_version = self.native_executor_version();
        if block_version == native_executor_version {
            self.native_validate_inner(block, options)
        } else {
            Err(ExecutorError::Other(format!(
                "Not supported version `{block_version}`. Expected version is `{}`",
                Self::VERSION
            )))
        }
    }

    #[cfg(feature = "wasm-executor")]
    fn wasm_produce_inner<TxSource>(
        &self,
        module: &wasmtime::Module,
        component: Components<TxSource>,
        options: ExecutionOptions,
        dry_run: bool,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let Components {
            header_to_produce,
            transactions_source,
            coinbase_recipient,
            gas_price,
        } = component;
        self.trace_block_version_warning(
            header_to_produce.state_transition_bytecode_version,
        );

        let source = Some(transactions_source);

        let block = Components {
            header_to_produce,
            transactions_source: (),
            coinbase_recipient,
            gas_price,
        };

        let previous_block_height = if !dry_run {
            block.header_to_produce.height().pred()
        } else {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/2062
            None
        };

        let instance_without_input =
            crate::instance::Instance::new(&self.engine).add_source(source)?;

        let instance_without_input = if let Some(previous_block_height) =
            previous_block_height
        {
            let storage = self.storage_view_provider.view_at(&previous_block_height)?;
            instance_without_input.add_storage(storage)?
        } else {
            let storage = self.storage_view_provider.latest_view()?;
            instance_without_input.add_storage(storage)?
        };

        let relayer = self.relayer_view_provider.latest_view()?;
        let instance_without_input = instance_without_input.add_relayer(relayer)?;

        let instance = if dry_run {
            instance_without_input.add_dry_run_input_data(block, options)?
        } else {
            instance_without_input.add_production_input_data(block, options)?
        };

        let output = instance.run(module)?;

        match output {
            fuel_core_wasm_executor::utils::ReturnType::V1(result) => result,
        }
    }

    #[cfg(feature = "wasm-executor")]
    fn wasm_validate_inner(
        &self,
        module: &wasmtime::Module,
        block: &Block,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        self.trace_block_version_warning(
            block.header().state_transition_bytecode_version,
        );
        let previous_block_height = block.header().height().pred();

        let instance = crate::instance::Instance::new(&self.engine).no_source()?;

        let instance = if let Some(previous_block_height) = previous_block_height {
            let storage = self.storage_view_provider.view_at(&previous_block_height)?;
            instance.add_storage(storage)?
        } else {
            let storage = self.storage_view_provider.latest_view()?;
            instance.add_storage(storage)?
        };

        let relayer = self.relayer_view_provider.latest_view()?;
        let instance = instance
            .add_relayer(relayer)?
            .add_validation_input_data(block, options)?;

        let output = instance.run(module)?;

        match output {
            fuel_core_wasm_executor::utils::ReturnType::V1(result) => {
                Ok(result?.into_validation_result())
            }
        }
    }

    fn native_produce_inner<TxSource>(
        &self,
        block: Components<TxSource>,
        options: ExecutionOptions,
        dry_run: bool,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let previous_block_height = if !dry_run {
            block.header_to_produce.height().pred()
        } else {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/2062
            None
        };
        let relayer = self.relayer_view_provider.latest_view()?;

        if let Some(previous_block_height) = previous_block_height {
            let database = self.storage_view_provider.view_at(&previous_block_height)?;
            ExecutionInstance::new(relayer, database, options)
                .produce_without_commit(block, dry_run)
        } else {
            let database = self.storage_view_provider.latest_view()?;
            ExecutionInstance::new(relayer, database, options)
                .produce_without_commit(block, dry_run)
        }
    }

    fn native_validate_inner(
        &self,
        block: &Block,
        options: ExecutionOptions,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        let previous_block_height = block.header().height().pred();
        let relayer = self.relayer_view_provider.latest_view()?;

        if let Some(previous_block_height) = previous_block_height {
            let database = self.storage_view_provider.view_at(&previous_block_height)?;
            ExecutionInstance::new(relayer, database, options)
                .validate_without_commit(block)
        } else {
            let database = self.storage_view_provider.latest_view()?;
            ExecutionInstance::new(relayer, database, options)
                .validate_without_commit(block)
        }
    }

    /// Attempt to fetch and validate an uploaded WASM module.
    #[cfg(feature = "wasm-executor")]
    pub fn validate_uploaded_wasm(
        &self,
        wasm_root: &Bytes32,
    ) -> Result<(), UpgradableError> {
        self.get_module_by_root_and_validate(*wasm_root).map(|_| ())
    }

    /// Find an uploaded WASM blob by it's hash and validate it.
    /// This is a slow operation, and the cached result should be used if possible,
    /// for instancy by calling `get_module` below.
    #[cfg(feature = "wasm-executor")]
    fn get_module_by_root_and_validate(
        &self,
        bytecode_root: Bytes32,
    ) -> Result<wasmtime::Module, UpgradableError> {
        let view = StructuredStorage::new(
            self.storage_view_provider
                .latest_view()
                .map_err(ExecutorError::from)?,
        );
        let uploaded_bytecode = view
            .storage::<UploadedBytecodes>()
            .get(&bytecode_root)
            .map_err(ExecutorError::from)?
            .ok_or(not_found!(UploadedBytecodes))
            .map_err(ExecutorError::from)?;

        let fuel_core_types::fuel_vm::UploadedBytecode::Completed(bytecode) =
            uploaded_bytecode.as_ref()
        else {
            return Err(UpgradableError::IncompleteUploadedBytecode(bytecode_root))
        };

        wasmtime::Module::new(&self.engine, bytecode)
            .map_err(|e| UpgradableError::InvalidWasm(e.to_string()))
    }

    /// Returns the compiled WASM module of the state transition function.
    ///
    /// Note: The method compiles the WASM module if it is not cached.
    ///     It is a long process to call this method, which can block the thread.
    #[cfg(feature = "wasm-executor")]
    fn get_module(
        &self,
        version: fuel_core_types::blockchain::header::StateTransitionBytecodeVersion,
    ) -> ExecutorResult<wasmtime::Module> {
        let guard = self.cached_modules.lock();
        if let Some(module) = guard.get(&version) {
            return Ok(module.clone());
        }
        drop(guard);

        let view = StructuredStorage::new(
            self.storage_view_provider
                .latest_view()
                .map_err(ExecutorError::from)?,
        );
        let bytecode_root = *view
            .storage::<StateTransitionBytecodeVersions>()
            .get(&version)
            .map_err(ExecutorError::from)?
            .ok_or(not_found!(StateTransitionBytecodeVersions))
            .map_err(ExecutorError::from)?;

        let module = self
            .get_module_by_root_and_validate(bytecode_root)
            .map_err(|err| match err {
                UpgradableError::InvalidWasm(_) => ExecutorError::Other(format!("Attempting to load invalid wasm bytecode, version={version}. Current version is `{}`", Self::VERSION)),
                UpgradableError::IncompleteUploadedBytecode(_) => ExecutorError::Other(format!("Attempting to load wasm bytecode failed since the upload is incomplete, version={version}. Current version is `{}`", Self::VERSION)),
                UpgradableError::ExecutorError(err) => err
            })?;

        self.cached_modules.lock().insert(version, module.clone());
        Ok(module)
    }
}

#[allow(clippy::cast_possible_truncation)]
#[allow(unexpected_cfgs)] // for cfg(coverage)
#[cfg(test)]
mod test {
    #[cfg(coverage)]
    use ntest as _; // Only used outside cdg(coverage)

    use super::*;
    use fuel_core_storage::{
        kv_store::Value,
        structured_storage::test::InMemoryStorage,
        tables::ConsensusParametersVersions,
        transactional::WriteTransaction,
        Result as StorageResult,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::{
            block::{
                Block,
                PartialFuelBlock,
            },
            header::{
                ApplicationHeader,
                ConsensusHeader,
                PartialBlockHeader,
                StateTransitionBytecodeVersion,
            },
            primitives::{
                DaBlockHeight,
                Empty,
            },
        },
        fuel_tx::{
            AssetId,
            Bytes32,
            Transaction,
        },
        services::relayer::Event,
        tai64::Tai64,
    };
    use std::collections::{
        BTreeMap,
        BTreeSet,
    };

    #[derive(Clone, Debug)]
    struct Storage(InMemoryStorage<Column>);

    impl AtomicView for Storage {
        type LatestView = InMemoryStorage<Column>;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(self.0.clone())
        }
    }

    impl HistoricalView for Storage {
        type Height = BlockHeight;
        type ViewAtHeight = Self::LatestView;

        fn latest_height(&self) -> Option<Self::Height> {
            None
        }

        fn view_at(&self, _: &Self::Height) -> StorageResult<Self::ViewAtHeight> {
            self.latest_view()
        }
    }

    impl KeyValueInspect for Storage {
        type Column = Column;

        fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
            self.0.get(key, column)
        }
    }

    impl Modifiable for Storage {
        fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
            self.0.commit_changes(changes)
        }
    }

    #[derive(Copy, Clone)]
    struct DisabledRelayer;

    impl RelayerPort for DisabledRelayer {
        fn enabled(&self) -> bool {
            false
        }

        fn get_events(&self, _: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
            unimplemented!()
        }
    }

    impl AtomicView for DisabledRelayer {
        type LatestView = Self;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(*self)
        }
    }

    // When this test fails, it is a sign that we need to increase the `Executor::VERSION`.
    #[test]
    fn version_check() {
        let crate_version = env!("CARGO_PKG_VERSION");
        let dashed_crate_version = crate_version.to_string().replace('.', "-");
        let mut seen_executor_versions =
            BTreeSet::<StateTransitionBytecodeVersion>::new();
        let seen_crate_versions = Executor::<Storage, DisabledRelayer>::CRATE_VERSIONS
            .iter()
            .map(|(crate_version, version)| {
                let executor_crate_version = crate_version.to_string().replace('-', ".");
                seen_executor_versions.insert(*version);
                (executor_crate_version, *version)
            })
            .collect::<BTreeMap<_, _>>();

        if let Some(expected_version) = seen_crate_versions.get(crate_version) {
            assert_eq!(
                *expected_version,
                Executor::<Storage, DisabledRelayer>::VERSION,
                "The version of the executor should be the same as in the `CRATE_VERSIONS` constant"
            );
        } else {
            let next_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            panic!(
                "New release {crate_version} is not mentioned in the `CRATE_VERSIONS` constant.\
                Please add the new entry `(\"{dashed_crate_version}\", {next_version})` \
                to the `CRATE_VERSIONS` constant."
            );
        }

        let last_crate_version = seen_crate_versions.last_key_value().unwrap().0.clone();
        assert_eq!(
            crate_version, last_crate_version,
            "The last version in the `CRATE_VERSIONS` constant \
                   should be the same as the current crate version."
        );

        assert_eq!(
            seen_executor_versions.len(),
            seen_crate_versions.len(),
            "Each executor version should be unique"
        );

        assert_eq!(
            seen_executor_versions.len() as u32 - 1,
            Executor::<Storage, DisabledRelayer>::VERSION,
            "The version of the executor should monotonically grow without gaps"
        );

        assert_eq!(
            seen_executor_versions.last().cloned().unwrap(),
            Executor::<Storage, DisabledRelayer>::VERSION,
            "The latest version of the executor should be the last version in the `CRATE_VERSIONS` constant"
        );
    }

    const CONSENSUS_PARAMETERS_VERSION: u32 = 0;

    fn storage() -> Storage {
        let mut storage = Storage(InMemoryStorage::default());
        let mut tx = storage.write_transaction();
        tx.storage_as_mut::<ConsensusParametersVersions>()
            .insert(&CONSENSUS_PARAMETERS_VERSION, &Default::default())
            .unwrap();
        tx.commit().unwrap();

        storage
    }

    fn valid_block(
        state_transition_bytecode_version: StateTransitionBytecodeVersion,
    ) -> Block {
        PartialFuelBlock::new(
            PartialBlockHeader {
                application: ApplicationHeader {
                    da_height: Default::default(),
                    consensus_parameters_version: CONSENSUS_PARAMETERS_VERSION,
                    state_transition_bytecode_version,
                    generated: Empty,
                },
                consensus: ConsensusHeader {
                    prev_root: Default::default(),
                    height: Default::default(),
                    time: Tai64::now(),
                    generated: Empty,
                },
            },
            vec![Transaction::mint(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                AssetId::BASE,
                Default::default(),
            )
            .into()],
        )
        .generate(&[], Bytes32::zeroed())
        .unwrap()
    }

    #[cfg(not(feature = "wasm-executor"))]
    mod native {
        use super::*;
        use crate::executor::Executor;
        use ntest as _;

        #[test]
        fn can_validate_block() {
            let storage = storage();
            let executor = Executor::native(storage, DisabledRelayer, Config::default());

            // Given
            let block = valid_block(Executor::<Storage, DisabledRelayer>::VERSION);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        #[test]
        fn validation_fails_because_of_versions_mismatch() {
            let storage = storage();
            let executor = Executor::native(storage, DisabledRelayer, Config::default());

            // Given
            let wrong_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let block = valid_block(wrong_version);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            result.expect_err("The validation should fail because of versions mismatch");
        }
    }

    #[cfg(feature = "wasm-executor")]
    #[allow(non_snake_case)]
    mod wasm {
        use super::*;
        use crate::{
            executor::Executor,
            WASM_BYTECODE,
        };
        use fuel_core_storage::tables::UploadedBytecodes;
        use fuel_core_types::fuel_vm::UploadedBytecode;

        #[test]
        fn can_validate_block__native_strategy() {
            let storage = storage();

            // Given
            let executor = Executor::native(storage, DisabledRelayer, Config::default());
            let block = valid_block(Executor::<Storage, DisabledRelayer>::VERSION);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        #[test]
        fn can_validate_block__wasm_strategy() {
            let storage = storage();

            // Given
            let executor = Executor::wasm(storage, DisabledRelayer, Config::default());
            let block = valid_block(Executor::<Storage, DisabledRelayer>::VERSION);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        #[test]
        fn validation_fails_because_of_versions_mismatch__native_strategy() {
            let storage = storage();

            // Given
            let executor = Executor::native(storage, DisabledRelayer, Config::default());
            let wrong_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let block = valid_block(wrong_version);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            result.expect_err("The validation should fail because of versions mismatch");
        }

        #[test]
        fn validation_fails_because_of_versions_mismatch__wasm_strategy() {
            let storage = storage();

            // Given
            let executor = Executor::wasm(storage, DisabledRelayer, Config::default());
            let wrong_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let block = valid_block(wrong_version);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            result.expect_err("The validation should fail because of versions mismatch");
        }

        fn storage_with_state_transition(
            next_version: StateTransitionBytecodeVersion,
        ) -> Storage {
            // Only FuelVM requires the Merkle root to match the corresponding bytecode
            // during uploading of it. The executor itself only uses a database to get the code,
            // and how bytecode appeared there is not the executor's responsibility.
            const BYTECODE_ROOT: Bytes32 = Bytes32::zeroed();

            let mut storage = storage();
            let mut tx = storage.write_transaction();
            tx.storage_as_mut::<StateTransitionBytecodeVersions>()
                .insert(&next_version, &BYTECODE_ROOT)
                .unwrap();
            tx.storage_as_mut::<UploadedBytecodes>()
                .insert(
                    &BYTECODE_ROOT,
                    &UploadedBytecode::Completed(WASM_BYTECODE.to_vec()),
                )
                .unwrap();
            tx.commit().unwrap();

            storage
        }

        #[test]
        fn can_validate_block_with_next_version__native_strategy() {
            // Given
            let next_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let storage = storage_with_state_transition(next_version);
            let executor = Executor::native(storage, DisabledRelayer, Config::default());
            let block = valid_block(next_version);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        #[test]
        fn can_validate_block_with_next_version__wasm_strategy() {
            // Given
            let next_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let storage = storage_with_state_transition(next_version);
            let executor = Executor::wasm(storage, DisabledRelayer, Config::default());
            let block = valid_block(next_version);

            // When
            let result = executor.validate(&block).map(|_| ());

            // Then
            assert_eq!(Ok(()), result);
        }

        // The test verifies that `Executor::get_module` method caches the compiled WASM module.
        // If it doesn't cache the modules, the test will fail with a timeout.
        #[test]
        #[cfg(not(coverage))] // Too slow for coverage
        #[ntest::timeout(60_000)]
        fn reuse_cached_compiled_module__native_strategy() {
            // Given
            let next_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let storage = storage_with_state_transition(next_version);
            let executor = Executor::native(storage, DisabledRelayer, Config::default());
            let block = valid_block(next_version);

            // When
            for _ in 0..1000 {
                let result = executor.validate(&block).map(|_| ());

                // Then
                assert_eq!(Ok(()), result);
            }
        }

        // The test verifies that `Executor::get_module` method caches the compiled WASM module.
        // If it doesn't cache the modules, the test will fail with a timeout.
        #[test]
        #[cfg(not(coverage))] // Too slow for coverage
        #[ntest::timeout(60_000)]
        fn reuse_cached_compiled_module__wasm_strategy() {
            // Given
            let next_version = Executor::<Storage, DisabledRelayer>::VERSION + 1;
            let storage = storage_with_state_transition(next_version);
            let executor = Executor::wasm(storage, DisabledRelayer, Config::default());
            let block = valid_block(next_version);

            // When
            for _ in 0..1000 {
                let result = executor.validate(&block).map(|_| ());

                // Then
                assert_eq!(Ok(()), result);
            }
        }
    }
}
