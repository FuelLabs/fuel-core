use std::{
    collections::HashMap,
    fmt::Debug,
};

use crate::{
    error::{
        CollisionReason,
        Error,
    },
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
    /// The cumulative of space used by a transaction and all of its children.
    pub dependents_cumulative_bytes_size: usize,
    /// Number of dependents
    pub number_txs_in_chain: usize,
}

pub type RemovedTransactions = Vec<ArcPoolTx>;

/// The storage trait is responsible for storing transactions.
/// It has to manage somehow the dependencies between transactions.
pub trait Storage {
    /// The index type used in the storage and allow other components to reference transactions.
    type StorageIndex: Copy + Debug;

    /// Store a transaction in the storage according to the dependencies.
    /// Returns the index of the stored transaction.
    fn store_transaction(
        &mut self,
        transaction: ArcPoolTx,
        dependencies: Vec<Self::StorageIndex>,
    ) -> Result<Self::StorageIndex, Error>;

    /// Check if a transaction could be stored in the storage. This shouldn't be expected to be called before store_transaction.
    /// Its just a way to perform some checks without storing the transaction.
    fn can_store_transaction(
        &self,
        transaction: &PoolTransaction,
        dependencies: &[Self::StorageIndex],
        collisions: &HashMap<Self::StorageIndex, Vec<CollisionReason>>,
    ) -> Result<(), Error>;

    /// Get the storage data by its index.
    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error>;

    /// Get the storage indexes of the dependencies of a transaction.
    fn get_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> Result<impl Iterator<Item = Self::StorageIndex>, Error>;

    /// Get the storage indexes of the dependents of a transaction.
    fn get_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> Result<impl Iterator<Item = Self::StorageIndex>, Error>;

    /// Get less worth subtree roots.
    fn get_worst_ratio_tip_gas_subtree_roots(
        &self,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Validate inputs of a transaction.
    fn validate_inputs(
        &self,
        transaction: &PoolTransaction,
        persistent_storage: &impl TxPoolPersistentStorage,
        utxo_validation: bool,
    ) -> Result<(), Error>;

    /// Collect the storage indexes of the transactions that are dependent on the given transaction.
    fn collect_transaction_dependencies(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    /// Remove a transaction from the storage by its index.
    /// The transaction is removed only if it has no dependencies.
    /// Doesn't remove the dependents.
    fn remove_transaction_without_dependencies(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<ArcPoolTx, Error>;

    /// Remove a transaction along with its dependents subtree.
    fn remove_transaction_and_dependents_subtree(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<RemovedTransactions, Error>;

    /// Count the number of transactions in the storage.
    fn count(&self) -> usize;
}
