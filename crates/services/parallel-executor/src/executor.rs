use crate::{
    config::Config,
    ports::TransactionsSource,
};
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
use fuel_core_upgradable_executor::executor::Executor as UpgradableExecutor;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};
use tokio::runtime::Runtime;

/// See the comment in `Executor::drop` for why we drop the inner runtime on a
/// dedicated OS thread instead of directly calling `shutdown_timeout`.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

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
            // Synchronously join this inner runtime's workers so they cannot
            // drop `Arc<rocksdb::PrimaryInstance>` clones after `main`
            // returns, which would race rocksdb's C++ static destructors at
            // libc `atexit`.
            //
            // We can't call `runtime.shutdown_timeout(...)` directly here
            // because this `Drop` usually runs on a tokio worker of the outer
            // runtime; tokio refuses to drop a runtime from an async context
            // and panics with "Cannot drop a runtime in a context where
            // blocking is not allowed". Moving the drop to a fresh OS thread
            // sidesteps that assertion — the calling thread just blocks on
            // `join`, which is a plain `pthread_join`.
            let handle = std::thread::spawn(move || {
                runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
            });
            let _ = handle.join();
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
    pub async fn produce_without_commit_with_source<TxSource>(
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
