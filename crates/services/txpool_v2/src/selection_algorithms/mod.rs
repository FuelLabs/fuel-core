use crate::storage::{
    RemovedTransactions,
    StorageData,
};

#[cfg(test)]
use fuel_core_types::services::txpool::ArcPoolTx;

pub mod ratio_tip_gas;

/// Constraints that the selection algorithm has to respect.
pub struct Constraints {
    /// Minimum gas price that all transaction must support.
    pub minimal_gas_price: u64,
    /// Maximum limit of gas that all selected transaction shouldn't exceed.
    pub max_gas: u64,
    /// Maximum number of transactions that can be selected.
    pub maximum_txs: u16,
    /// Maximum size of the block.
    pub maximum_block_size: u32,
}

/// The selection algorithm is responsible for selecting the best transactions to include in a block.
pub trait SelectionAlgorithm {
    /// The storage type of the selection algorithm.
    type Storage;
    /// The index that identifies a transaction in the storage.
    type StorageIndex;
    /// Given the constraints, the selection algorithm has to return the best list of transactions to include in a block.
    fn gather_best_txs(
        &mut self,
        constraints: Constraints,
        storage: &mut Self::Storage,
    ) -> RemovedTransactions;

    /// Update the selection algorithm with the new transaction that are executable.
    fn new_executable_transaction(
        &mut self,
        storage_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Get less worth transactions iterator
    fn get_less_worth_txs(&self) -> impl Iterator<Item = &Self::StorageIndex>;

    /// Inform the selection algorithm that a transaction was removed from the pool.
    fn on_removed_transaction(&mut self, storage_entry: &StorageData);

    #[cfg(test)]
    /// Asserts the integrity of the selection algorithm.
    fn assert_integrity(&self, expected_txs: &[ArcPoolTx]);
}
