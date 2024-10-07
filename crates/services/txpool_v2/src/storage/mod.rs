use std::{
    collections::HashSet,
    fmt::Debug,
    hash::Hash,
    time::Instant,
};

use crate::{
    error::Error,
    ports::TxPoolPersistentStorage,
};
use fuel_core_types::services::txpool::{
    ArcPoolTx,
    PoolTransaction,
};

pub mod checked_collision;
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
    pub number_dependents_in_chain: usize,
    /// The instant when the transaction was added to the pool.
    pub creation_instant: Instant,
}

pub type RemovedTransactions = Vec<StorageData>;

pub trait CheckedTransaction<StorageIndex> {
    /// Returns the underlying transaction.
    fn tx(&self) -> &ArcPoolTx;

    /// Unwraps the transaction.
    fn into_tx(self) -> ArcPoolTx;

    /// Returns the list of all dependencies of the transaction.
    fn all_dependencies(&self) -> &HashSet<StorageIndex>;
}

/// The storage trait is responsible for storing transactions.
/// It has to manage somehow the dependencies between transactions.
pub trait Storage {
    /// The index type used in the storage and allow other components to reference transactions.
    type StorageIndex: Copy + Debug + Eq + Hash;

    /// The checked type wraps the transaction inside to show that it was verified by the
    /// [`store_transaction`] method. Later checked transaction can be passed
    /// to the [`store_transaction`] method.
    type CheckedTransaction: CheckedTransaction<Self::StorageIndex>;

    /// Store a transaction in the storage according to the dependencies.
    /// Returns the index of the stored transaction.
    fn store_transaction(
        &mut self,
        checked_transaction: Self::CheckedTransaction,
        creation_instant: Instant,
    ) -> Self::StorageIndex;

    /// The function performs checks on the transaction and returns a checked transaction.
    ///
    /// [`Self::CheckedTransaction`] is required to call [`Self::store_transaction`].
    fn can_store_transaction(
        &self,
        transaction: ArcPoolTx,
    ) -> Result<Self::CheckedTransaction, Error>;

    /// Get the storage data by its index.
    fn get(&self, index: &Self::StorageIndex) -> Option<&StorageData>;

    /// Get direct dependents of a transaction.
    fn get_direct_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> impl Iterator<Item = Self::StorageIndex>;

    /// Returns `true` if the transaction has dependencies.
    fn has_dependencies(&self, index: &Self::StorageIndex) -> bool;

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

    /// Remove a transaction from the storage.
    fn remove_transaction(&mut self, index: Self::StorageIndex) -> Option<StorageData>;
}
