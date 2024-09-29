use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fmt::Debug,
    hash::Hash,
    time::Instant,
};

use crate::{
    error::{
        CollisionReason,
        Error,
    },
    ports::TxPoolPersistentStorage,
};
use fuel_core_types::services::txpool::PoolTransaction;

pub mod checked_collision;
pub mod graph;

#[derive(Debug)]
pub struct StorageData {
    /// The transaction.
    pub transaction: PoolTransaction,
    /// The cumulative tip of a transaction and all of its children.
    pub dependents_cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    pub dependents_cumulative_gas: u64,
    /// The cumulative of space used by a transaction and all of its children.
    pub dependents_cumulative_bytes_size: usize,
    /// Number of dependents
    pub number_dependents_in_chain: usize,
    /// The instant when the transaction was added to the pool.
    pub creation_instant: Instant,
}

pub type RemovedTransactions = Vec<StorageData>;

pub trait TransactionWithCollisions<StorageIndex> {
    /// Returns the instigator of the collision.
    fn tx(&self) -> &PoolTransaction;

    /// Returns the transactions that collide with the given transaction.
    fn colliding_transactions(&self) -> &HashMap<StorageIndex, Vec<CollisionReason>>;

    /// Unwraps the collision into a instigator transaction.
    fn into_instigator(self) -> PoolTransaction;
}

pub trait CheckedTransactionWithCollisions<StorageIndex>:
    TransactionWithCollisions<StorageIndex>
{
    /// Returns the list of all dependencies of the transaction.
    fn all_dependencies(&self) -> &HashSet<StorageIndex>;
}

/// The storage trait is responsible for storing transactions.
/// It has to manage somehow the dependencies between transactions.
pub trait Storage {
    /// The index type used in the storage and allow other components to reference transactions.
    type StorageIndex: Copy + Debug + Eq + Hash;
    type CheckedTransaction<Tx>: CheckedTransactionWithCollisions<Self::StorageIndex>
    where
        Tx: TransactionWithCollisions<Self::StorageIndex>;

    /// Store a transaction in the storage according to the dependencies.
    /// Returns the index of the stored transaction.
    fn store_transaction<Tx>(
        &mut self,
        checked_transaction: Self::CheckedTransaction<Tx>,
        creation_instant: Instant,
    ) -> Self::StorageIndex
    where
        Tx: TransactionWithCollisions<Self::StorageIndex>;

    /// Check if a transaction could be stored in the storage. This shouldn't be expected to be called before store_transaction.
    /// Its just a way to perform some checks without storing the transaction.
    fn can_store_transaction<Tx>(
        &self,
        transaction: Tx,
    ) -> Result<Self::CheckedTransaction<Tx>, Error>
    where
        Tx: TransactionWithCollisions<Self::StorageIndex>;

    /// Get the storage data by its index.
    fn get(&self, index: &Self::StorageIndex) -> Option<&StorageData>;

    /// Get the storage indexes of the direct dependencies of a transaction.
    fn get_direct_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex>;

    /// Get the storage indexes of the direct dependents of a transaction.
    fn get_direct_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex>;

    /// Validate inputs of a transaction.
    fn validate_inputs(
        &self,
        transaction: &PoolTransaction,
        persistent_storage: &impl TxPoolPersistentStorage,
        utxo_validation: bool,
    ) -> Result<(), Error>;

    /// Remove a transaction along with its dependents subtree.
    fn remove_transaction_and_dependents_subtree(
        &mut self,
        index: Self::StorageIndex,
    ) -> RemovedTransactions;

    /// Count the number of transactions in the storage.
    fn count(&self) -> usize;
}
