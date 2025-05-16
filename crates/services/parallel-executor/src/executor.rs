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
    fuel_tx::{
        ContractId,
        Transaction,
    },
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
        let mut partial_block =
            PartialFuelBlock::new(components.header_to_produce, vec![]);
        let prev_height = components.header_to_produce.height().pred();

        let mut data = ExecutionData::new();
        let mut memory = MemoryInstance::new();
        let mut view = self
            .scheduler
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;

        let da_changes = if prev_height
            .and_then(|height| {
                view.get_da_height_by_l2_height(&height)
                    .map_err(SchedulerError::StorageError)
                    .ok()
            })
            .flatten()
            .filter(|&da_height| da_height != components.header_to_produce.da_height)
            .is_some()
        {
            self.process_l1_txs(
                &mut partial_block,
                components.coinbase_recipient,
                &mut data,
                &mut memory,
                &mut view,
            )?
        } else {
            StorageChanges::default()
        };

        let mut components = components;
        let res = self
            .scheduler
            .run(
                &mut components,
                da_changes,
                BlockConstraints {
                    block_gas_limit: 30_000_000 - data.used_gas,
                    total_execution_time: Duration::from_millis(300),
                    block_transaction_size_limit: u32::MAX - data.used_size,
                    block_transaction_count_limit: u16::MAX - data.tx_count,
                },
                data.into(),
            )
            .await?;

        let (execution_data, storage_changes) = self.produce_mint_tx(
            &mut components,
            &mut partial_block,
            res,
            &mut memory,
            &mut view,
        )?;

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
            storage_changes,
        ))
    }

    fn process_l1_txs(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        coinbase_contract_id: ContractId,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        view: &mut View,
    ) -> Result<StorageChanges, SchedulerError> {
        let mut storage_tx = StorageTransaction::transaction(
            view,
            ConflictPolicy::Fail,
            Default::default(),
        );
        self.scheduler
            .executor
            .process_l1_txs(
                partial_block,
                coinbase_contract_id,
                &mut storage_tx,
                execution_data,
                memory,
            )
            .map_err(SchedulerError::ExecutionError)?;
        Ok(StorageChanges::Changes(storage_tx.into_changes()))
    }

    fn produce_mint_tx<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        partial_block: &mut PartialFuelBlock,
        mut scheduler_res: SchedulerExecutionResult,
        memory: &mut MemoryInstance,
        view: &mut View,
    ) -> Result<(ExecutionData, StorageChanges), SchedulerError> {
        let tx_count = u16::try_from(scheduler_res.transactions.len())
            .expect("previously checked; qed");

        partial_block.header = scheduler_res.header;
        partial_block.transactions = scheduler_res.transactions;

        let mut tx_changes = StorageTransaction::transaction(
            view,
            ConflictPolicy::Fail,
            Default::default(),
        );

        let mut execution_data = ExecutionData {
            coinbase: scheduler_res.coinbase,
            skipped_transactions: scheduler_res.skipped_txs,
            events: scheduler_res.events,
            changes: Default::default(),
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
                partial_block,
                components,
                &mut tx_changes,
                &mut execution_data,
                memory,
            )
            .map_err(SchedulerError::ExecutionError)?;

        let storage_changes = match scheduler_res.changes {
            StorageChanges::Changes(changes) => {
                StorageChanges::ChangesList(vec![changes, tx_changes.into_changes()])
            }
            StorageChanges::ChangesList(ref mut changes_list) => {
                changes_list.push(tx_changes.into_changes());
                scheduler_res.changes
            }
        };

        Ok((execution_data, storage_changes))
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
