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
    validator::{
        self,
        Validator,
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
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::PartialBlockHeader,
    },
    fuel_tx::{
        ContractId,
        Transaction,
        field::{
            InputContract,
            MintGasPrice,
        },
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

/// Default block execution constraints
mod defaults {
    use super::*;

    pub const BLOCK_GAS_LIMIT: u64 = 30_000_000;
    pub const EXECUTION_TIME_LIMIT: Duration = Duration::from_millis(300);
    pub const BLOCK_TX_SIZE_LIMIT: u32 = u32::MAX;
    pub const BLOCK_TX_COUNT_LIMIT: u16 = u16::MAX;
}

pub struct Executor<S, R, P> {
    scheduler: Scheduler<R, S, P>,
    validator: Validator,
}

impl<S, R, P> Executor<S, R, P> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        preconfirmation_sender: P,
        config: Config,
    ) -> Self {
        let scheduler = Scheduler::new(
            config.clone(),
            relayer_view_provider,
            storage_view_provider,
            preconfirmation_sender,
        );

        let validator = Validator::new(config);

        Self {
            scheduler,
            validator,
        }
    }
}

impl<S, R, P, View> Executor<S, R, P>
where
    R: RelayerPort + Clone + Send + 'static,
    P: PreconfirmationSenderPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: KeyValueInspect<Column = Column> + Storage + Send + Sync + 'static,
{
    /// Produces the block and returns the result of the execution without committing the changes.
    pub async fn produce_without_commit_with_source<TxSource>(
        &mut self,
        mut components: Components<TxSource>,
    ) -> Result<Uncommitted<ExecutionResult, StorageChanges>, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        // Initialize execution state
        let mut partial_block =
            PartialFuelBlock::new(components.header_to_produce, vec![]);
        let mut execution_data = ExecutionData::new();
        let mut memory = MemoryInstance::new();

        // Process L1 transactions if needed
        let da_changes = self
            .process_da_if_needed(
                &mut partial_block,
                &mut execution_data,
                &mut memory,
                &components,
            )
            .await?;

        // Run parallel scheduler for L2 transactions
        let scheduler_result = self
            .run_scheduler(&mut components, da_changes, execution_data)
            .await?;

        // Finalize block with mint transaction
        self.finalize_block(&mut components, scheduler_result, &mut memory)
    }

    async fn validate_block(
        mut self,
        block: &Block,
        block_storage_tx: StorageTransaction<View>,
    ) -> Result<validator::ValidationResult, SchedulerError> {
        let mut data = ExecutionData::new();

        let partial_header = PartialBlockHeader::from(block.header());
        let mut partial_block = PartialFuelBlock::new(partial_header, vec![]);
        let transactions = block.transactions();
        let mut memory = MemoryInstance::new();

        let (gas_price, coinbase_contract_id) =
            Self::get_coinbase_info_from_mint_tx(transactions)?;

        let block_storage_tx = self.process_l1_txs(
            &mut partial_block,
            coinbase_contract_id,
            &mut data,
            &mut memory,
            block_storage_tx,
        )?;
        let processed_l1_tx_count = partial_block.transactions.len();

        let components = Components {
            header_to_produce: partial_block.header,
            transactions_source: transactions
                .to_vec()
                .into_iter()
                .skip(processed_l1_tx_count),
            coinbase_recipient: coinbase_contract_id,
            gas_price,
        };

        let executed_block_result = self
            .validator
            .recreate_block(components, block_storage_tx)
            .await?;

        if let Some((_, error)) = executed_block_result.skipped_transactions.first() {
            return Err(SchedulerError::SkippedTransaction(error.clone()));
        }

        if executed_block_result.block_id == block.header().id() {
            Ok(executed_block_result)
        } else {
            Err(SchedulerError::BlockMismatch)
        }
    }

    fn get_coinbase_info_from_mint_tx(
        transactions: &[Transaction],
    ) -> Result<(u64, ContractId), SchedulerError> {
        if let Some(Transaction::Mint(mint)) = transactions.last() {
            Ok((*mint.gas_price(), mint.input_contract().contract_id))
        } else {
            Err(SchedulerError::MintMissing)
        }
    }

    /// Process DA changes if the DA height has changed
    async fn process_da_if_needed(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        components: &Components<impl TransactionsSource>,
    ) -> Result<Changes, SchedulerError> {
        let prev_height = components.header_to_produce.height().pred();
        let view = self
            .scheduler
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;

        let should_process_da = prev_height
            .and_then(|height| {
                view.get_da_height_by_l2_height(&height)
                    .map_err(SchedulerError::StorageError)
                    .ok()
            })
            .flatten()
            .filter(|&da_height| da_height != components.header_to_produce.da_height)
            .is_some();

        if should_process_da {
            let storage_tx = StorageTransaction::transaction(
                view,
                ConflictPolicy::Fail,
                Default::default(),
            );
            let storage_tx = self.process_l1_txs(
                partial_block,
                components.coinbase_recipient,
                execution_data,
                memory,
                storage_tx,
            )?;
            Ok(storage_tx.into_changes())
        } else {
            Ok(Changes::default())
        }
    }

    /// Process L1 transactions
    fn process_l1_txs(
        &mut self,
        partial_block: &mut PartialFuelBlock,
        coinbase_contract_id: ContractId,
        execution_data: &mut ExecutionData,
        memory: &mut MemoryInstance,
        mut storage_tx: StorageTransaction<View>,
    ) -> Result<StorageTransaction<View>, SchedulerError> {
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

        Ok(storage_tx)
    }

    /// Run the parallel executor for L2 transactions
    async fn run_scheduler<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        da_changes: Changes,
        execution_data: ExecutionData,
    ) -> Result<SchedulerExecutionResult, SchedulerError>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let block_constraints = self.calculate_block_constraints(&execution_data)?;

        self.scheduler
            .run(
                components,
                da_changes,
                block_constraints,
                execution_data.into(),
            )
            .await
    }

    /// Calculate remaining block constraints after L1 execution
    fn calculate_block_constraints(
        &self,
        execution_data: &ExecutionData,
    ) -> Result<BlockConstraints, SchedulerError> {
        let gas_limit = defaults::BLOCK_GAS_LIMIT
            .checked_sub(execution_data.used_gas)
            .ok_or_else(|| {
                SchedulerError::InternalError(
                    "L1 transactions exhausted block gas limit".to_string(),
                )
            })?;

        let tx_size_limit = defaults::BLOCK_TX_SIZE_LIMIT
            .checked_sub(execution_data.used_size)
            .ok_or_else(|| {
                SchedulerError::InternalError(
                    "L1 transactions exhausted block size limit".to_string(),
                )
            })?;

        let tx_count_limit = defaults::BLOCK_TX_COUNT_LIMIT
            .checked_sub(execution_data.tx_count)
            .ok_or_else(|| {
                SchedulerError::InternalError(
                    "L1 transactions exhausted block transaction count".to_string(),
                )
            })?;

        Ok(BlockConstraints {
            block_gas_limit: gas_limit,
            total_execution_time: defaults::EXECUTION_TIME_LIMIT,
            block_transaction_size_limit: tx_size_limit,
            block_transaction_count_limit: tx_count_limit,
        })
    }

    /// Finalize the block by adding mint transaction and generating the final block
    fn finalize_block<TxSource>(
        &mut self,
        components: &mut Components<TxSource>,
        scheduler_result: SchedulerExecutionResult,
        memory: &mut MemoryInstance,
    ) -> Result<Uncommitted<ExecutionResult, StorageChanges>, SchedulerError>
    where
        TxSource: TransactionsSource,
    {
        let view = self
            .scheduler
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;

        // Produce mint transaction (pass the entire scheduler_result)
        let (execution_data, storage_changes, partial_block) =
            self.produce_mint_tx(components, scheduler_result, memory, view)?;

        // Generate final block
        let block = partial_block
            .generate(
                &execution_data.message_ids,
                Default::default(),
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
        memory: &mut MemoryInstance,
        view: View,
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

        let tx_count = transactions.len() as u16;

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
            event_inbox_root: Default::default(),
            used_gas,
            used_size,
        };

        self.scheduler
            .executor
            .produce_mint_tx(
                &mut partial_block,
                components,
                &mut tx_changes,
                &mut execution_data,
                memory,
            )
            .map_err(SchedulerError::ExecutionError)?;

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

    #[cfg(feature = "wasm-executor")]
    pub fn validate_uploaded_wasm(
        &self,
        _wasm_root: &Bytes32,
    ) -> Result<(), UpgradableError> {
        unimplemented!("WASM validation not implemented yet");
    }

    pub fn dry_run(
        &self,
        _component: Components<Vec<Transaction>>,
        _utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        unimplemented!("Dry run not implemented yet");
    }
}
