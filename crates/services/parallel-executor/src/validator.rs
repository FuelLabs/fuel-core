use std::{
    collections::HashMap,
    sync::Arc,
};

use dependency_graph::DependencyGraph;
use fuel_core_executor::{
    executor::{
        BlockExecutor,
        ExecutionData,
        ExecutionOptions,
    },
    ports::{
        MaybeCheckedTransaction,
        PreconfirmationSenderPort,
        RelayerPort,
    },
};
use fuel_core_storage::{
    StorageAsRef,
    column::Column,
    kv_store::KeyValueInspect,
    tables::ConsensusParametersVersions,
    transactional::{
        Changes,
        ConflictPolicy,
        IntoTransaction,
        StorageTransaction,
    },
};
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
        header::{
            BlockHeader,
            PartialBlockHeader,
        },
        primitives::BlockId,
    },
    fuel_tx::{
        ConsensusParameters,
        ContractId,
        MessageId,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    fuel_vm::{
        checked_transaction::IntoChecked,
        interpreter::MemoryInstance,
    },
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutionError,
            Event,
            TransactionExecutionStatus,
        },
        relayer,
    },
};
use futures::{
    StreamExt,
    stream::FuturesUnordered,
};

pub(crate) mod dependency_graph;

use crate::{
    config::Config,
    ports::Storage,
    scheduler::SchedulerError,
    tx_waiter::NoWaitTxs,
};

#[derive(Clone)]
struct NoPreconfirmationSender;

impl PreconfirmationSenderPort for NoPreconfirmationSender {
    fn send(
        &self,
        _preconfirmations: Vec<
            fuel_core_types::services::preconfirmation::Preconfirmation,
        >,
    ) -> impl Future<Output = ()> + Send {
        futures::future::ready(())
    }

    fn try_send(
        &self,
        _preconfirmations: Vec<
            fuel_core_types::services::preconfirmation::Preconfirmation,
        >,
    ) -> Vec<fuel_core_types::services::preconfirmation::Preconfirmation> {
        vec![]
    }
}

pub struct Validator {
    config: Config,
}

pub struct TransactionExecutionResult {
    pub changes: Changes,
    pub tx_index: u16,
    pub events: Vec<Event>,
    pub status: TransactionExecutionStatus,
    pub skipped_transactions: Vec<(TxId, ExecutionError)>,
    pub transaction: Transaction,
    pub message_ids: Vec<MessageId>,
    // TODO: Add coins to verify their dependency
}

/// Inspiration from: https://github.com/FuelLabs/fuel-core/blob/85b2356d510a30cffaa8be7015203bb8ac30fee6/crates/types/src/services/executor.rs#L86
/// and https://github.com/FuelLabs/fuel-core/blob/7fccb06d6a5c971fc3f649ed1e509e13e57eb9ca/crates/services/parallel-executor/src/executor.rs#L751
pub struct ValidationResult {
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
    /// The list of all events generated during the execution of the block.
    pub events: Vec<Event>,
    /// Block id
    pub block_id: BlockId,
    /// Skipped transactions
    pub skipped_transactions: Vec<(TxId, ExecutionError)>,
}

impl Validator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn validate_block<S, D, R>(
        &self,
        relayer: R,
        components: Components<S>,
        block_storage_tx: StorageTransaction<D>,
        block: &fuel_core_types::blockchain::block::Block,
    ) -> Result<ValidationResult, SchedulerError>
    where
        D: KeyValueInspect<Column = Column> + Storage + Send + Sync + 'static,
        S: Iterator<Item = Transaction>,
        R: RelayerPort + Clone + Send + 'static,
    {
        let executed_block_result = self
            .recreate_block(relayer, components, block_storage_tx)
            .await?;

        // validation 1: ensure that there are no skipped transactions
        if let Some((_, error)) = executed_block_result.skipped_transactions.first() {
            return Err(SchedulerError::SkippedTransaction(error.clone()));
        }

        // validation 2: ensure that the block id is valid
        if executed_block_result.block_id != block.header().id() {
            return Err(SchedulerError::BlockMismatch)
        }

        Ok(executed_block_result)
    }

    async fn recreate_block<S, D, R>(
        &self,
        relayer: R,
        components: Components<S>,
        block_storage_tx: StorageTransaction<D>,
    ) -> Result<ValidationResult, SchedulerError>
    where
        D: KeyValueInspect<Column = Column> + Storage + Send + Sync + 'static,
        S: Iterator<Item = Transaction>,
        R: RelayerPort + Clone + Send + 'static,
    {
        let mut execution_results = HashMap::new();
        let consensus_parameters = block_storage_tx
            .storage::<ConsensusParametersVersions>()
            .get(&components.consensus_parameters_version())
            .map_err(|e| SchedulerError::InternalError(e.to_string()))?
            .ok_or(SchedulerError::ConsensusParametersNotFound(
                components.consensus_parameters_version(),
            ))?
            .into_owned();
        let executor = BlockExecutor::new(
            relayer,
            ExecutionOptions {
                forbid_fake_coins: false,
                backtrace: false,
            },
            consensus_parameters,
            NoWaitTxs,
            NoPreconfirmationSender,
            true, // dry run
        )
        .map_err(|e| {
            SchedulerError::InternalError(format!("Failed to create executor: {e}"))
        })?;
        let mut highest_id: u16 = 0;
        let storage_tx = Arc::new(block_storage_tx);
        let mut workers_running = 0;
        let mut dependency_graph =
            DependencyGraph::new(components.transactions_source.size_hint().0);

        // add all transactions to the dependency graph
        dependency_graph.add_transactions(components.transactions_source.enumerate());

        let mut current_execution_tasks: FuturesUnordered<
            tokio::task::JoinHandle<TransactionExecutionResult>,
        > = FuturesUnordered::new();

        while !dependency_graph.is_empty() {
            // Check if we can spawn more tasks
            if workers_running < self.config.number_of_cores.get() {
                if let Some(tx_id) = dependency_graph.pop_ready_transaction() {
                    let (transaction, changes) = dependency_graph
                        .extract_transaction_object(tx_id)
                        .expect("Transaction should exist in the graph");
                    let tx_id = tx_id.try_into().map_err(|_| {
                        SchedulerError::InternalError(
                            "Transaction index out of bounds".to_string(),
                        )
                    })?;
                    let storage_tx = storage_tx
                        .clone()
                        .into_transaction()
                        .with_policy(ConflictPolicy::Overwrite)
                        .with_changes(changes);
                    // If a transaction is ready to be executed, spawn a new task
                    self.spawn_execution_task(
                        executor.clone(),
                        components.header_to_produce.clone(),
                        components.gas_price,
                        components.coinbase_recipient,
                        tx_id,
                        transaction,
                        storage_tx,
                    );
                    workers_running += 1;
                    if highest_id < tx_id {
                        highest_id = tx_id;
                    }
                    continue;
                }
            }
            // If no transactions are ready to be executed, wait for the next task to complete
            if let Some(res) = current_execution_tasks.next().await {
                workers_running -= 1;
                match res {
                    Ok(result) => {
                        // Process the result of the executed transaction
                        dependency_graph.mark_tx_as_executed(
                            result.tx_index as usize,
                            result.changes.clone(),
                        );
                        execution_results.insert(result.tx_index, result);
                    }
                    Err(e) => {
                        // Handle the error from the execution task
                        return Err(SchedulerError::InternalError(e.to_string()));
                    }
                }
            }
        }

        // Wait for all tasks to complete
        while let Some(res) = current_execution_tasks.next().await {
            match res {
                Ok(result) => {
                    dependency_graph.mark_tx_as_executed(
                        result.tx_index as usize,
                        result.changes.clone(),
                    );
                    execution_results.insert(result.tx_index, result);
                }
                Err(e) => {
                    // Handle the error from the execution task
                    return Err(SchedulerError::InternalError(e.to_string()));
                }
            }
        }

        let mut validation_result = ValidationResult {
            tx_status: Vec::new(),
            events: Vec::new(),
            block_id: Default::default(),
            skipped_transactions: Vec::new(),
        };
        let mut transactions = Vec::with_capacity(highest_id as usize);
        let mut message_ids = Vec::new();
        // TODO: Use coin dependency checker from scheduler
        // Collect the results from the executed transactions

        for tx_idx in 0..highest_id {
            if let Some(result) = execution_results.remove(&tx_idx) {
                validation_result.tx_status.push(result.status);
                validation_result.events.extend(result.events);
                validation_result
                    .skipped_transactions
                    .extend(result.skipped_transactions);
                transactions.push(result.transaction);
                message_ids.extend(result.message_ids);
            }
        }

        let block = components
            .header_to_produce
            .generate(
                &transactions,
                &message_ids,
                Default::default(),
                #[cfg(feature = "fault-proving")]
                &Default::default(),
            )
            .map_err(|e| {
                SchedulerError::InternalError(format!("Failed to generate block: {e}"))
            })?;

        validation_result.block_id = block.id();
        Ok(validation_result)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_execution_task<D, R>(
        &self,
        executor: BlockExecutor<R, NoWaitTxs, NoPreconfirmationSender>,
        header_to_produce: PartialBlockHeader,
        gas_price: u64,
        coinbase_recipient: ContractId,
        tx_idx: u16,
        transaction: Transaction,
        mut storage_tx: StorageTransaction<D>,
    ) -> tokio::task::JoinHandle<Result<TransactionExecutionResult, ExecutionError>>
    where
        D: KeyValueInspect<Column = Column> + Send + Sync + 'static,
        R: RelayerPort + Clone + Send + 'static,
    {
        // Spawn a new task to execute the transaction
        tokio::spawn(async move {
            let mut execution_data = ExecutionData {
                tx_count: tx_idx,
                ..Default::default()
            };
            let mut partial_block = PartialFuelBlock::new(header_to_produce, vec![]);
            executor.execute_transaction_and_commit(
                &mut partial_block,
                &mut storage_tx,
                &mut execution_data,
                MaybeCheckedTransaction::Transaction(transaction),
                gas_price,
                coinbase_recipient,
                &mut MemoryInstance::new(),
            )?;
            Ok(TransactionExecutionResult {
                changes: execution_data.changes,
                events: execution_data.events,
                status: execution_data
                    .tx_status
                    .pop()
                    .expect("At least one transaction status should be present"),
                skipped_transactions: execution_data.skipped_transactions,
                transaction: partial_block
                    .transactions
                    .pop()
                    .expect("Transaction should be present"),
                message_ids: execution_data.message_ids,
                tx_index: tx_idx,
            })
        })
    }
}
