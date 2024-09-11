use std::{
    collections::HashSet,
    fmt::Debug,
    time::Instant,
};

use crate::{
    collision_manager::CollisionReason,
    error::Error,
    ports::TxPoolDb,
};
use fuel_core_types::services::txpool::PoolTransaction;

pub mod graph;

#[derive(Debug)]
pub struct StorageData {
    /// The transaction.
    pub transaction: PoolTransaction,
    /// The cumulative tip of a transaction and all of its children.
    pub cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    pub cumulative_gas: u64,
    /// Number of dependents
    pub number_txs_in_chain: u64,
    /// Submitted time
    pub submitted_time: Instant,
}

pub type RemovedTransactions = Vec<PoolTransaction>;

/// The storage trait is responsible for storing transactions.
/// It has to manage somehow the dependencies between transactions.
pub trait Storage {
    /// The index type used in the storage and allow other components to reference transactions.
    type StorageIndex: Copy + Debug;

    /// Store a transaction in the storage according to the dependencies and collisions.
    /// Returns the index of the stored transaction and the transactions that were removed from the storage in favor of the new transaction.
    fn store_transaction(
        &mut self,
        transaction: PoolTransaction,
        dependencies: Vec<Self::StorageIndex>,
        collided_transactions: Vec<Self::StorageIndex>,
    ) -> Result<(Self::StorageIndex, RemovedTransactions), Error>;

    /// Get the storage data by its index.
    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error>;

    /// Get the storage indexes of the dependencies of a transaction.
    fn get_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Get the storage indexes of the dependents of a transaction.
    fn get_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Collect the storage indexes of the transactions that are dependent on the given transaction.
    /// The collisions can be useful as they implies that some verfications had already been done.
    /// Returns the storage indexes of the dependencies transactions.
    fn collect_dependencies_transactions(
        &self,
        transaction: &PoolTransaction,
        collisions: HashSet<CollisionReason>,
        db: &impl TxPoolDb,
        utxo_validation: bool,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Remove a transaction from the storage by its index.
    /// Doesn't remove the dependents.
    fn remove_transaction(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<StorageData, Error>;

    /// Count the number of transactions in the storage.
    fn count(&self) -> u64;
}
