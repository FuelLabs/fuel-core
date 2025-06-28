use crate::{
    config::Config,
    ports::TransactionsSource,
    scheduler::{
        Scheduler,
        SchedulerError,
        SchedulerExecutionResult,
    },
    tx_waiter::NoWaitTxs,
    txs_ext::TxCoinbaseExt,
    validator::{
        self,
        Validator,
    },
};
use fuel_core_executor::{
    executor::{
        BlockExecutor,
        ExecutionData,
        ExecutionOptions,
    },
    ports::{
        PreconfirmationSenderPort,
        RelayerPort,
    },
};
use fuel_core_storage::{
    StorageAsRef,
    column::Column,
    kv_store::KeyValueInspect,
    structured_storage::StructuredStorage,
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
    },
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        StorageChanges,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::PartialBlockHeader,
    },
    fuel_tx::{
        Bytes32,
        ConsensusParameters,
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

pub struct Executor<S, R, P> {
    config: Config,
    relayer: R,
    storage: S,
    preconfirmation_sender: P,
    memory_pool: Option<Vec<MemoryInstance>>,
}

impl<S, R, P> Executor<S, R, P>
where
    R: Clone,
{
    pub fn new(
        storage_view_provider: S,
        relayer: R,
        preconfirmation_sender: P,
        config: Config,
    ) -> Self {
        Self {
            memory_pool: Some(vec![MemoryInstance::new(); config.number_of_cores.get()]),
            config,
            relayer,
            storage: storage_view_provider,
            preconfirmation_sender,
        }
    }
}

impl<S, R, P, View> Executor<S, R, P>
where
    R: RelayerPort + Clone + Send + 'static,
    P: PreconfirmationSenderPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
{
    /// Produces the block and returns the result of the execution without committing the changes.
    pub async fn produce_without_commit_with_source<TxSource>(
        &mut self,
        mut components: Components<TxSource>,
        maximum_execution_time: Duration,
    ) -> Result<Uncommitted<ExecutionResult, StorageChanges>, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        // Initialize execution state
        let mut partial_block =
            PartialFuelBlock::new(components.header_to_produce, vec![]);
        let mut execution_data = ExecutionData::new();
        let view = self.storage.latest_view()?;
        let structured_storage = StructuredStorage::new(view);
        let consensus_parameters = {
            structured_storage
                .storage::<ConsensusParametersVersions>()
                .get(&components.header_to_produce.consensus_parameters_version)?
                .ok_or_else(|| {
                    SchedulerError::InternalError(
                        "Consensus parameters not found".to_string(),
                    )
                })?
                .into_owned()
        };

        // Initialize block executor
        let mut executor = BlockExecutor::new(
            self.relayer.clone(),
            ExecutionOptions {
                forbid_unauthorized_inputs: true,
                forbid_fake_utxo: false,
                backtrace: false,
            },
            consensus_parameters.clone(),
            NoWaitTxs,
            self.preconfirmation_sender.clone(),
            false, // not dry run
        )
        .map_err(|e| {
            SchedulerError::InternalError(format!("Failed to create executor: {e}"))
        })?;

        // Process L1 transactions if needed
        let (da_changes, event_inbox_root) = self
            .process_da_if_needed(
                &mut partial_block,
                &mut execution_data,
                &mut MemoryInstance::new(),
                &components,
                &mut executor,
                structured_storage,
            )
            .await?;

        // Run parallel scheduler for L2 transactions
        let memory_pool =
            self.memory_pool
                .take()
                .ok_or(SchedulerError::InternalError(
                    "Memory pool is not initialized".to_string(),
                ))?;
        let scheduler_result = self
            .run_scheduler(
                &mut components,
                da_changes,
                execution_data,
                executor.clone(),
                memory_pool,
                consensus_parameters,
                maximum_execution_time,
            )
            .await?;

        // Finalize block with mint transaction
        self.finalize_block(
            &mut components,
            scheduler_result,
            event_inbox_root,
            &mut executor,
        )
    }

    /// Process DA changes if the DA height has changed
    async fn process_da_if_needed(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        components: &Components<impl TransactionsSource>,
        executor: &mut BlockExecutor<R, NoWaitTxs, P>,
        structured_storage: StructuredStorage<View>,
    ) -> Result<(Changes, Bytes32), SchedulerError> {
        let Some(prev_height) = components.header_to_produce.height().pred() else {
            return Ok(Default::default());
        };

        let prev_da = structured_storage
            .storage::<FuelBlocks>()
            .get(&prev_height)?
            .ok_or_else(|| {
                SchedulerError::InternalError("Previous block not found".to_string())
            })?
            .header()
            .da_height();

        let mut storage_tx = StorageTransaction::transaction(
            structured_storage.into_storage(),
            ConflictPolicy::Fail,
            Default::default(),
        );

        if prev_da != components.header_to_produce.da_height {
            let event_inbox_root = self.process_l1_txs(
                partial_block,
                components.coinbase_recipient,
                execution_data,
                memory,
                &mut storage_tx,
                executor,
            )?;
            Ok((storage_tx.into_changes(), event_inbox_root))
        } else {
            Ok(Default::default())
        }
    }

    /// Process L1 transactions
    fn process_l1_txs(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        coinbase_contract_id: ContractId,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        storage_tx: &mut StorageTransaction<View>,
        executor: &mut BlockExecutor<R, NoWaitTxs, P>,
    ) -> Result<Bytes32, SchedulerError> {
        executor.process_l1_txs(
            partial_block,
            coinbase_contract_id,
            storage_tx,
            execution_data,
            memory,
        )?;

        Ok(execution_data.event_inbox_root)
    }

    /// Run the parallel executor for L2 transactions
    #[allow(clippy::too_many_arguments)]
    async fn run_scheduler<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        da_changes: Changes,
        execution_data: ExecutionData,
        executor: BlockExecutor<R, NoWaitTxs, P>,
        memory_pool: Vec<MemoryInstance>,
        consensus_parameters: ConsensusParameters,
        maximum_execution_time: Duration,
    ) -> Result<SchedulerExecutionResult, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let scheduler = Scheduler::new(
            self.config.clone(),
            self.storage.clone(),
            executor,
            memory_pool,
            consensus_parameters,
            maximum_execution_time,
        )?;

        let (res, memory_instance) = scheduler
            .run(components, da_changes, execution_data.into())
            .await?;

        // Restore memory pool to be re-used
        self.memory_pool = Some(memory_instance);
        Ok(res)
    }

    /// Finalize the block by adding mint transaction and generating the final block
    fn finalize_block<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        scheduler_result: SchedulerExecutionResult,
        event_inbox_root: Bytes32,
        executor: &mut BlockExecutor<R, NoWaitTxs, P>,
    ) -> Result<Uncommitted<ExecutionResult, StorageChanges>, SchedulerError>
    where
        TxSource: TransactionsSource,
    {
        let view = self.storage.latest_view()?;

        // Produce mint transaction (pass the entire scheduler_result)
        let (execution_data, storage_changes, partial_block) = self.produce_mint_tx(
            components,
            scheduler_result,
            event_inbox_root,
            view,
            executor,
        )?;

        // Generate final block
        let block = partial_block
            .generate(
                &execution_data.message_ids,
                event_inbox_root,
                #[cfg(feature = "fault-proving")]
                &Default::default(),
            )
            .map_err(|e| {
                SchedulerError::InternalError(format!("Block generation failed: {}", e))
            })?;

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

    /// Produce mint transaction and merge storage changes
    fn produce_mint_tx<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        scheduler_res: SchedulerExecutionResult,
        event_inbox_root: Bytes32,
        view: View,
        executor: &mut BlockExecutor<R, NoWaitTxs, P>,
    ) -> Result<(ExecutionData, StorageChanges, PartialFuelBlock), SchedulerError> {
        // needed to avoid partial move
        let SchedulerExecutionResult {
            header,
            transactions,
            events,
            message_ids,
            skipped_txs,
            transactions_status,
            mut changes,
            used_gas,
            used_size,
            coinbase,
        } = scheduler_res;

        let tx_count = u16::try_from(transactions.len()).map_err(|_| {
            SchedulerError::InternalError("Too many transactions".to_string())
        })?;

        let mut partial_block = PartialFuelBlock {
            header,
            transactions,
        };

        let mut tx_changes = StorageTransaction::transaction(
            view,
            ConflictPolicy::Fail,
            Default::default(),
        );

        let mut execution_data = ExecutionData {
            coinbase,
            skipped_transactions: skipped_txs,
            events,
            changes: Default::default(),
            message_ids,
            tx_count,
            tx_status: transactions_status,
            found_mint: false,
            event_inbox_root,
            used_gas,
            used_size,
        };

        executor.produce_mint_tx(
            &mut partial_block,
            components,
            &mut tx_changes,
            &mut execution_data,
            &mut MemoryInstance::new(),
        )?;

        let storage_changes = match changes {
            StorageChanges::Changes(changes) => {
                StorageChanges::ChangesList(vec![changes, tx_changes.into_changes()])
            }
            StorageChanges::ChangesList(ref mut changes_list) => {
                changes_list.push(tx_changes.into_changes());
                changes
            }
        };

        Ok((execution_data, storage_changes, partial_block))
    }

    pub fn validate(
        &self,
        _block: &Block,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        unimplemented!("Parallel validation not implemented yet");
    }

    pub fn dry_run(
        &self,
        _component: Components<Vec<Transaction>>,
        _utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        unimplemented!("Dry run not implemented yet");
    }
}

impl<S, R, View> Executor<S, R, validator::NoPreconfirmationSender>
where
    R: RelayerPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
{
    async fn validate_block(
        mut self,
        block: &Block,
        mut block_storage_tx: StorageTransaction<View>,
    ) -> Result<validator::ValidationResult, SchedulerError> {
        let mut data = ExecutionData::new();

        let partial_header = PartialBlockHeader::from(block.header());
        let mut partial_block = PartialFuelBlock::new(partial_header, vec![]);
        let transactions = block.transactions();
        let mut memory = MemoryInstance::new();
        let consensus_parameters = {
            block_storage_tx
                .storage::<ConsensusParametersVersions>()
                .get(&block.header().consensus_parameters_version())?
                .ok_or_else(|| {
                    SchedulerError::InternalError(
                        "Consensus parameters not found".to_string(),
                    )
                })?
                .into_owned()
        };
        // Initialize block executor
        let mut executor = BlockExecutor::new(
            self.relayer.clone(),
            ExecutionOptions {
                forbid_unauthorized_inputs: true,
                forbid_fake_utxo: false,
                backtrace: false,
            },
            consensus_parameters.clone(),
            NoWaitTxs,
            validator::NoPreconfirmationSender,
            true, // dry run
        )
        .map_err(|e| {
            SchedulerError::InternalError(format!("Failed to create executor: {e}"))
        })?;
        let validator = Validator::new(self.config.clone(), executor.clone());

        let (gas_price, coinbase_contract_id) = transactions.coinbase()?;

        let _event_root = self.process_l1_txs(
            &mut partial_block,
            coinbase_contract_id,
            &mut data,
            &mut memory,
            &mut block_storage_tx,
            &mut executor,
        )?;
        let processed_l1_tx_count = partial_block.transactions.len();

        let components = Components {
            header_to_produce: partial_block.header,
            transactions_source: transactions.iter().cloned().skip(processed_l1_tx_count),
            coinbase_recipient: coinbase_contract_id,
            gas_price,
        };

        let executed_block_result = validator
            .validate_block(components, block_storage_tx, block)
            .await?;

        Ok(executed_block_result)
    }
}
