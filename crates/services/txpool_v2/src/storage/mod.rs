use std::{
    collections::HashSet,
    fmt::Debug,
    time::Instant,
};

use crate::{
    collision_manager::CollisionReason,
    error::Error,
    ports::TxPoolPersistentStorage,
};
use fuel_core_types::services::txpool::{
    ArcPoolTx,
    PoolTransaction,
};

pub mod graph;

#[derive(Debug)]
pub struct StorageData {
    /// The transaction.
    /// We are forced to take an arc here as we need to be able to send a transaction that still exists here to p2p and API.
    pub transaction: ArcPoolTx,
    /// The cumulative tip of a transaction and all of its children.
    pub dependents_cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    pub dependents_cumulative_gas: u64,
    /// Number of dependents
    pub number_txs_in_chain: u64,
    /// Submitted time
    pub submitted_time: Instant,
}

pub type RemovedTransactions = Vec<ArcPoolTx>;

/// The storage trait is responsible for storing transactions.
/// It has to manage somehow the dependencies between transactions.
pub trait Storage {
    /// The index type used in the storage and allow other components to reference transactions.
    type StorageIndex: Copy + Debug;

    /// Store a transaction in the storage according to the dependencies and collisions.
    /// Returns the index of the stored transaction and the transactions that were removed from the storage in favor of the new transaction.
    fn store_transaction(
        &mut self,
        transaction: ArcPoolTx,
        dependencies: &[Self::StorageIndex],
        collided_transactions: &[Self::StorageIndex],
    ) -> Result<(Self::StorageIndex, RemovedTransactions), Error>;

    /// Check if a transaction could be stored in the storage. This shouldn't be expected to be called before store_transaction.
    /// Its just a way to perform some checks without storing the transaction.
    fn can_store_transaction(
        &self,
        transaction: &PoolTransaction,
        dependencies: &[Self::StorageIndex],
        collided_transactions: &[Self::StorageIndex],
    ) -> Result<(), Error>;

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
    /// The collisions can be useful as they implies that some verifications had already been done.
    /// Returns the storage indexes of the dependencies transactions.
    fn validate_inputs_and_collect_dependencies(
        &self,
        transaction: &PoolTransaction,
        collisions: HashSet<CollisionReason>,
        persistent_storage: &impl TxPoolPersistentStorage,
        utxo_validation: bool,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Remove a transaction from the storage by its index.
    /// Doesn't remove the dependents.
    fn remove_transaction(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<StorageData, Error>;

    /// Count the number of transactions in the storage.
    fn count(&self) -> usize;
}
