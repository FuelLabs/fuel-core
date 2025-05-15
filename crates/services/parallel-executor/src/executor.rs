use crate::{
    config::Config,
    ports::{
        Storage,
        TransactionsSource,
    },
    scheduler::{
        BlockConstraints,
        Scheduler,
        SchedulerError,
        SchedulerExecutionResult,
    },
};
use fuel_core_executor::{
    executor::ExecutionData,
    ports::{
        PreconfirmationSenderPort,
        RelayerPort,
    },
};
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        StorageChanges,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::block::{
        Block,
        PartialFuelBlock,
    },
    fuel_tx::Transaction,
    fuel_vm::interpreter::MemoryInstance,
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
        // TODO: More higher error type
    ) -> Result<Uncommitted<ExecutionResult, StorageChanges>, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        // TODO: Manage DA
        let mut components = components;
        let res = self
            .scheduler
            .run(
                &mut components,
                StorageChanges::default(),
                BlockConstraints {
                    block_gas_limit: 30_000_000,
                    total_execution_time: Duration::from_millis(300),
                    block_transaction_size_limit: u32::MAX,
                    block_transaction_count_limit: u16::MAX,
                },
            )
            .await?;

        let (partial_block, execution_data, mint_changes) =
            self.produce_mint_tx(&mut components, res)?;

        let block = partial_block
            .generate(
                &execution_data.message_ids,
                Default::default(),
                #[cfg(feature = "fault-proving")]
                &Default::default(),
            )
            .unwrap();

        Ok(Uncommitted::new(
            ExecutionResult {
                block,
                skipped_transactions: execution_data.skipped_transactions,
                events: execution_data.events,
                tx_status: execution_data.tx_status,
            },
            StorageChanges::ChangesList(vec![execution_data.changes, mint_changes]),
        ))
    }

    fn produce_mint_tx<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        scheduler_res: SchedulerExecutionResult,
    ) -> Result<(PartialFuelBlock, ExecutionData, Changes), SchedulerError> {
        let tx_count = u16::try_from(scheduler_res.transactions.len())
            .expect("previously checked; qed");
        let mut block: PartialFuelBlock = PartialFuelBlock {
            header: scheduler_res.header,
            transactions: scheduler_res.transactions,
        };

        let mut memory = MemoryInstance::new();
        let view = self
            .scheduler
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;
        let mut tx_changes = StorageTransaction::transaction(
            view,
            ConflictPolicy::Fail,
            Default::default(),
        );
        let mut execution_data = ExecutionData {
            coinbase: scheduler_res.coinbase,
            skipped_transactions: scheduler_res.skipped_txs,
            events: scheduler_res.events,
            changes: scheduler_res.changes.try_into().unwrap(),
            message_ids: scheduler_res.message_ids,
            tx_count,
            tx_status: scheduler_res.transactions_status,
            found_mint: false,
            event_inbox_root: Default::default(),
            used_gas: scheduler_res.used_gas,
            used_size: scheduler_res.used_size,
        };

        self.scheduler
            .executor
            .produce_mint_tx(
                &mut block,
                components,
                &mut tx_changes,
                &mut execution_data,
                &mut memory,
            )
            .map_err(SchedulerError::ExecutionError)?;

        Ok((block, execution_data, tx_changes.into_changes()))
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
