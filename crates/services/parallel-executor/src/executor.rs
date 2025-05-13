use crate::{
    config::Config,
    ports::{
        Storage,
        TransactionsSource,
    },
    scheduler::{
        BlockConstraints,
        Scheduler,
    },
};
use fuel_core_executor::ports::{
    PreconfirmationSenderPort,
    RelayerPort,
};
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Changes,
        StorageChanges,
    },
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    services::{
        Uncommitted,
        block_producer::Components,
        executor::{
            // ExecutionResult,
            Result as ExecutorResult,
            TransactionExecutionStatus,
            ValidationResult,
        },
    },
};
use std::time::Duration;

#[cfg(feature = "wasm-executor")]
use fuel_core_upgradable_executor::error::UpgradableError;

#[cfg(feature = "wasm-executor")]
use fuel_core_types::fuel_merkle::common::Bytes32;

pub struct Executor<S, R, P> {
    scheduler: Scheduler<R, S, P>,
}

impl<S, R, P> Executor<S, R, P> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        preconfirmation_sender: P,
        config: Config,
    ) -> Self {
        let scheduler = Scheduler::new(
            config,
            relayer_view_provider,
            storage_view_provider,
            preconfirmation_sender,
        );

        Self { scheduler }
    }
}

impl<S, R, P, View> Executor<S, R, P>
where
    R: RelayerPort + Clone + Send + 'static,
    P: PreconfirmationSenderPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: KeyValueInspect<Column = Column> + Storage + Send,
{
    /// Produces the block and returns the result of the execution without committing the changes.
    pub async fn produce_without_commit_with_source<TxSource>(
        &mut self,
        components: Components<TxSource>,
    )
    // TODO : ExecutorResult<Uncommitted<ExecutionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        self.scheduler
            .run(
                components,
                StorageChanges::default(),
                BlockConstraints {
                    block_gas_limit: u64::MAX,
                    total_execution_time: Duration::from_secs(1),
                    block_transaction_size_limit: u32::MAX,
                    block_transaction_count_limit: u16::MAX,
                },
            )
            .await
            .unwrap();
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
