use std::{
    collections::HashMap,
    sync::{
        Arc,
        RwLock,
    },
};

use dependency_graph::DependencyGraph;
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    transactional::StorageTransaction,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    fuel_tx::{
        Transaction,
        TxId,
    },
    services::{
        block_producer::Components,
        executor::{
            Event,
            TransactionExecutionStatus,
        },
    },
};

pub(crate) mod dependency_graph;

use crate::{
    config::Config,
    scheduler::SchedulerError,
};

pub struct Validator {
    config: Config,
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
        D: KeyValueInspect<Column = Column>,
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
        D: KeyValueInspect<Column = Column>,
        S: Iterator<Item = Transaction>,
    {
        let mut dependency_graph =
            DependencyGraph::new(components.transactions_source.size_hint().0);

        // add all transactions to the dependency graph
        dependency_graph.add_transactions(components.transactions_source.enumerate());

        // create execution batches
        let execution_batches =
            self.create_execution_batches(&mut dependency_graph).await?;

        // execute batches in parallel
        let batch_results = self.execute_batches_parallel(execution_batches).await?;

        // merge results and return final validation result
        self.merge_batch_results(batch_results).await
    }

    async fn create_execution_batches(
        &self,
        _dependency_graph: &mut DependencyGraph,
    ) -> Result<Vec<()>, SchedulerError> {
        todo!()
    }

    async fn execute_batches_parallel(
        &self,
        _execution_batches: Vec<()>,
    ) -> Result<Vec<()>, SchedulerError> {
        todo!()
    }

    async fn merge_batch_results(
        &self,
        _batch_results: Vec<()>,
    ) -> Result<ValidationResult, SchedulerError> {
        todo!()
    }
}
