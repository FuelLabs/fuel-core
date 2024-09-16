use std::{
    collections::HashSet,
    fmt::Debug,
};

use fuel_core_types::{
    fuel_merkle::storage,
    fuel_tx::{
        BlobId,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::{
    error::Error,
    ports::TxPoolPersistentStorage,
    storage::StorageData,
};

pub mod basic;

/// The reason why a transaction collides with another.
/// It also contains additional information about the collision.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum CollisionReason {
    Coin(UtxoId),
    Blob(BlobId),
    Message(Nonce),
    ContractCreation(ContractId),
}

/// Contains all the information about the collisions of a transaction.
#[derive(Default, Debug)]
pub struct Collisions<Idx> {
    pub reasons: HashSet<CollisionReason>,
    pub colliding_txs: Vec<Idx>,
}

impl<Idx> Collisions<Idx> {
    /// Create a new empty collision information.
    pub fn new() -> Self {
        Self {
            reasons: HashSet::default(),
            colliding_txs: vec![],
        }
    }
}

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;

    /// Collect all the transactions that collide with the given transaction.
    /// It returns an error if the transaction is less worthy than the colliding transactions.
    /// It returns the information about the collisions.
    fn collect_colliding_transactions(
        &self,
        transaction: &PoolTransaction,
        storage: &Self::Storage,
    ) -> Result<Collisions<Self::StorageIndex>, Error>;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction: &PoolTransaction,
        transaction_id: Self::StorageIndex,
    ) -> Result<(), Error>;

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error>;
}
