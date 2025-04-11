use crate::config::Config;
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    services::{
        Uncommitted,
        block_producer::Components,
        executor::{
            ExecutionResult,
            Result as ExecutorResult,
            TransactionExecutionStatus,
            ValidationResult,
        },
    },
};
use fuel_core_upgradable_executor::{
    executor::Executor as UpgradableExecutor,
    native_executor::ports::TransactionsSource,
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        RwLock,
    },
};
use tokio::runtime::Runtime;

#[cfg(feature = "wasm-executor")]
use fuel_core_upgradable_executor::error::UpgradableError;

#[cfg(feature = "wasm-executor")]
use fuel_core_types::fuel_merkle::common::Bytes32;

pub struct Executor<S, R> {
    _executor: Arc<RwLock<UpgradableExecutor<S, R>>>,
    runtime: Option<Runtime>,
    _number_of_cores: NonZeroUsize,
}

// Shutdown the tokio runtime to avoid panic if executor is already
//   used from another tokio runtime
impl<S, R> Drop for Executor<S, R> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<S, R> Executor<S, R> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        let executor = UpgradableExecutor::new(
            storage_view_provider,
            relayer_view_provider,
            config.executor_config,
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.number_of_cores.get())
            .enable_all()
            .build()
            .unwrap();
        let number_of_cores = config.number_of_cores;

        Self {
            _executor: Arc::new(RwLock::new(executor)),
            runtime: Some(runtime),
            _number_of_cores: number_of_cores,
        }
    }
}

impl<S, R> Executor<S, R> {
    /// Produces the block and returns the result of the execution without committing the changes.
    pub fn produce_without_commit_with_source<TxSource>(
        &self,
        _components: Components<TxSource>,
    ) -> ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        unimplemented!("Not implemented yet");
    }

    pub fn validate(
        &self,
        _block: &Block,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        unimplemented!("Not implemented yet");
    }

    #[cfg(feature = "wasm-executor")]
    pub fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), UpgradableError> {
        unimplemented!("Not implemented yet");
    }

    /// Executes the block and returns the result of the execution without committing
    /// the changes in the dry run mode.
    pub fn dry_run(
        &self,
        _component: Components<Vec<Transaction>>,
        _utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        unimplemented!("Not implemented yet");
    }
}
