use crate::{
    config::Config,
    memory::MemoryPool,
    ports::TransactionsSource,
    scheduler::{
        Scheduler,
        SchedulerError,
        SchedulerExecutionResult,
    },
    tx_waiter::NoWaitTxs,
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
    blockchain::block::{
        Block,
        PartialFuelBlock,
    },
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
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
use tokio::runtime::Runtime;

pub struct Executor<S, R, P> {
    config: Config,
    relayer: R,
    storage: S,
    preconfirmation_sender: P,
    memory_pool: MemoryPool,
    runtime: Option<Runtime>,
}

impl<S, R, P> Drop for Executor<S, R, P> {
    fn drop(&mut self) {
        // Shutdown the tokio runtime to avoid panic if executor is already
        // used from another tokio runtime
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<S, R, P> Executor<S, R, P> {
    pub fn new(
        storage_view_provider: S,
        relayer: R,
        preconfirmation_sender: P,
        config: Config,
    ) -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.number_of_cores.get())
            .enable_all()
            .build()?;

        Ok(Self {
            memory_pool: MemoryPool::new(),
            runtime: Some(runtime),
            config,
            relayer,
            storage: storage_view_provider,
            preconfirmation_sender,
        })
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
                allow_syscall: false,
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
        let scheduler_result = self
            .run_scheduler(
                &components,
                da_changes,
                execution_data,
                executor.clone(),
                consensus_parameters,
                maximum_execution_time,
            )
            .await?;
        tracing::warn!(
            "Scheduler finished with {} transactions, {} events, and {} skipped transactions",
            scheduler_result.transactions.len(),
            scheduler_result.events.len(),
            scheduler_result.skipped_txs.len()
        );

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
            // In the case of genesis block, there are no DA changes to process
            return Ok((
                Default::default(),
                MerkleRootCalculator::new().root().into(),
            ));
        };

        let prev_block = structured_storage
            .storage::<FuelBlocks>()
            .get(&prev_height)?
            .ok_or_else(|| {
                SchedulerError::InternalError("Previous block not found".to_string())
            })?;

        if prev_block.header().da_height() != components.header_to_produce.da_height {
            let (storage_tx, event_inbox_root) = self.process_l1_txs(
                partial_block,
                components.coinbase_recipient,
                execution_data,
                memory,
                structured_storage.into_storage(),
                executor,
            )?;
            Ok((storage_tx.into_changes(), event_inbox_root))
        } else {
            // No DA changes to process
            Ok((
                Default::default(),
                MerkleRootCalculator::new().root().into(),
            ))
        }
    }

    /// Process L1 transactions
    fn process_l1_txs(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        coinbase_contract_id: ContractId,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        view: View,
        executor: &mut BlockExecutor<R, NoWaitTxs, P>,
    ) -> Result<(StorageTransaction<View>, Bytes32), SchedulerError> {
        let mut storage_tx = StorageTransaction::transaction(
            view,
            ConflictPolicy::Fail,
            Default::default(),
        );

        executor.process_l1_txs(
            partial_block,
            coinbase_contract_id,
            &mut storage_tx,
            execution_data,
            memory,
        )?;

        Ok((storage_tx, execution_data.event_inbox_root))
    }

    /// Run the parallel executor for L2 transactions
    #[allow(clippy::too_many_arguments)]
    async fn run_scheduler<TxSource>(
        &mut self,
        components: &Components<TxSource>,
        da_changes: Changes,
        execution_data: ExecutionData,
        executor: BlockExecutor<R, NoWaitTxs, P>,
        consensus_parameters: ConsensusParameters,
        maximum_execution_time: Duration,
    ) -> Result<SchedulerExecutionResult, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let runtime = self.runtime.as_ref().expect(
            "Scheduler runtime \
                is only removed on `drop`",
        );
        let scheduler = Scheduler::new(
            components,
            self.config.clone(),
            self.storage.clone(),
            executor,
            runtime,
            self.memory_pool.clone(),
            consensus_parameters,
            maximum_execution_time,
        )?;

        let res = scheduler
            .run(
                &components.transactions_source,
                da_changes,
                execution_data.into(),
            )
            .await?;

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
        tracing::warn!(
            "Produced mint transaction with {} gas and {} size",
            execution_data.used_gas,
            execution_data.used_size
        );

        // Generate final block
        let res = partial_block
            .generate(
                &execution_data.message_ids,
                event_inbox_root,
                #[cfg(feature = "fault-proving")]
                &Default::default(),
            )
            .map_err(|e| {
                SchedulerError::InternalError(format!("Block generation failed: {}", e))
            });

        match &res {
            Ok(_) => tracing::warn!("Block generated successfully"),
            Err(e) => tracing::warn!("Failed to generate block: {}", e),
        }
        let block = res?;

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

        let tx_count = u32::try_from(transactions.len()).map_err(|_| {
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
