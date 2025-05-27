use std::{
    collections::HashMap,
    sync::{
        Arc,
        RwLock,
    },
};

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
        let mut tx_batches = vec![vec![]; self.config.number_of_cores.get()];
        let mut all_batches_results: Arc<RwLock<HashMap<u64, ()>>> =
            Arc::new(RwLock::new(HashMap::default()));
        let mut max_batch_id = 0;
        // Splitting tx into batches for now it's a single tx batch.
        // This will not work because of the possible usage of the same contract in multiple parallel transactions.
        for (batch_id, tx) in components.transactions_source.enumerate() {
            let batch_index = batch_id % self.config.number_of_cores.get();
            tx_batches[batch_index].push((batch_id, vec![tx]));
            max_batch_id = batch_id;
        }

        // Execute each batch in parallel.
        let mut handles = vec![];
        for (batch_id, batches) in tx_batches.into_iter().enumerate() {
            let all_batches_results = Arc::clone(&all_batches_results);

            let handle = tokio::spawn(async move {
                // Verify and executes each transaction in the batch.
                all_batches_results
                    .write()
                    .unwrap()
                    .insert(batch_id as u64, ());
            });

            handles.push(handle);
        }

        // Consolidate results from all batches.
        for handle in handles {
            handle.await.map_err(|_| {
                SchedulerError::InternalError("Failed to join batch execution".into())
            })?;
        }

        let mut validation_result = ValidationResult {
            tx_status: vec![],
            events: vec![],
            block_id: BlockId::default(),
            skipped_transactions: vec![],
        };
        for i in 0..=max_batch_id {
            let batch_result =
                all_batches_results.read().unwrap().get(&(i as u64)).ok_or(
                    SchedulerError::InternalError(format!(
                        "Batch result for batch {} not found",
                        i
                    )),
                )?;

            // Populate validation result
        }

        // Generate block id

        Ok(validation_result)
    }
}
