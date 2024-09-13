use std::fmt::Debug;

use fuel_core_types::services::txpool::PoolTransaction;

use crate::{
    error::Error,
    storage::StorageData,
};

pub mod ratio_tip_gas;

/// Constraints that the selection algorithm has to respect.
pub struct Constraints {
    pub max_gas: u64,
}

/// The selection algorithm is responsible for selecting the best transactions to include in a block.
pub trait SelectionAlgorithm {
    /// The storage type of the selection algorithm.
    type Storage;
    /// The index that identifies a transaction in the storage.
    type StorageIndex: Copy + Debug;
    /// Given the constraints, the selection algorithm has to return the best list of transactions to include in a block.
    fn gather_best_txs(
        &mut self,
        constraints: Constraints,
        storage: &Self::Storage,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Update the selection algorithm with the new transactions that are executable.
    fn new_executable_transactions(
        &mut self,
        transactions_ids: Vec<Self::StorageIndex>,
        storage: &Self::Storage,
    ) -> Result<(), Error>;

    /// Inform the selection algorithm that a transaction was removed from the pool.
    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error>;
}
