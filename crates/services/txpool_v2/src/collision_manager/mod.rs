use std::{
    collections::{
        HashMap,
        HashSet,
    },
    fmt::Debug,
};

use fuel_core_types::{
    fuel_tx::{
        BlobId,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::{
    error::{
        CollisionReason,
        Error,
    },
    storage::StorageData,
};

pub mod basic;
pub mod collision;

pub trait Collision<StorageIndex> {
    /// Returns the instigator of the collision.
    fn tx(&self) -> &PoolTransaction;

    /// Returns the transactions that collide with the given transaction.
    fn colliding_transactions(&self) -> &HashMap<StorageIndex, Vec<CollisionReason>>;

    /// Converts the collision into a collection of storage indexes.
    fn into_colliding_storage(self) -> Vec<StorageIndex>;
}

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;
    /// The collision type returned by the collision manager.
    type Collision<'a>: Collision<Self::StorageIndex>;

    /// Finds collision for the transaction. Returns `Self::Collision`, if find any.
    fn find_collision<'a>(
        &self,
        transaction: &'a PoolTransaction,
    ) -> Option<Self::Collision<'a>>;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);
}
