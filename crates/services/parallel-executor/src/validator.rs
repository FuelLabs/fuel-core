use std::sync::Arc;

use dependency_graph::DependencyGraph;
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
    blockchain::primitives::BlockId,
    fuel_tx::{
        ConsensusParameters,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    fuel_vm::checked_transaction::IntoChecked,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutionError,
            Event,
            TransactionExecutionStatus,
        },
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
};

pub struct Validator {
    config: Config,
}

pub struct TransactionExecutionResult {
    pub changes: Changes,
    pub tx_index: usize,
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
    pub skipped_transactions: Vec<(TxId, String)>,
}

impl Validator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn validate_block<S, D>(
        &self,
        components: Components<S>,
        block_storage_tx: StorageTransaction<D>,
        block: &fuel_core_types::blockchain::block::Block,
    ) -> Result<ValidationResult, SchedulerError>
    where
        D: KeyValueInspect<Column = Column> + Storage,
        S: Iterator<Item = Transaction>,
    {
        let executed_block_result =
            self.recreate_block(components, block_storage_tx).await?;

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

    async fn recreate_block<S, D>(
        &self,
        components: Components<S>,
        block_storage_tx: StorageTransaction<D>,
    ) -> Result<ValidationResult, SchedulerError>
    where
        D: KeyValueInspect<Column = Column> + Storage,
        S: Iterator<Item = Transaction>,
    {
        let consensus_parameters = block_storage_tx
            .storage::<ConsensusParametersVersions>()
            .get(&components.consensus_parameters_version())
            .map_err(|e| SchedulerError::InternalError(e.to_string()))?
            .ok_or(SchedulerError::ConsensusParametersNotFound(
                components.consensus_parameters_version(),
            ))?
            .into_owned();
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
                    let storage_tx = storage_tx
                        .clone()
                        .into_transaction()
                        .with_policy(ConflictPolicy::Overwrite)
                        .with_changes(changes);
                    // If a transaction is ready to be executed, spawn a new task
                    self.spawn_execution_task(
                        *components.header_to_produce.height(),
                        consensus_parameters.clone(),
                        tx_id,
                        transaction,
                        storage_tx,
                    );
                    workers_running += 1;
                    continue;
                }
            }
            // If no transactions are ready to be executed, wait for the next task to complete
            if let Some(res) = current_execution_tasks.next().await {
                workers_running -= 1;
                match res {
                    Ok(result) => {
                        // Process the result of the executed transaction
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
                    // Process the result of the executed transaction
                }
                Err(e) => {
                    // Handle the error from the execution task
                    return Err(SchedulerError::InternalError(e.to_string()));
                }
            }
        }

        todo!()
    }

    fn spawn_execution_task<D>(
        &self,
        block_height: BlockHeight,
        consensus_params: ConsensusParameters,
        tx_idx: usize,
        transaction: Transaction,
        storage_tx: StorageTransaction<D>,
    ) -> tokio::task::JoinHandle<Result<TransactionExecutionResult, ExecutionError>>
    where
        D: KeyValueInspect<Column = Column>,
    {
        // Spawn a new task to execute the transaction
        tokio::spawn(async move {
            // Verify the transaction
            // TODO: Maybe do it directly in executor
            let checked_tx = transaction.into_checked(block_height, &consensus_params)?;
            // Execute the transaction

            Ok(TransactionExecutionResult {
                changes: Changes::default(), // Replace with actual changes

                tx_index: tx_idx,
            })
        })
    }
}
