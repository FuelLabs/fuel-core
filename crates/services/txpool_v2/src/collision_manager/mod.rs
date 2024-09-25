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

use crate::error::{
    CollisionReason,
    Error,
};

pub mod basic;

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;

    /// Collect the transaction that collide with the given transaction.
    /// Returns a list of storage indexes of the colliding transactions and the reason of the collision.
    fn collect_colliding_transactions(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<HashMap<Self::StorageIndex, Vec<CollisionReason>>, Error>;

    /// Determine if the collisions allow the transaction to be stored.
    /// Returns the reason of the collision if the transaction cannot be stored.
    fn can_store_transaction(
        &self,
        transaction: &PoolTransaction,
        has_dependencies: bool,
        colliding_transactions: &HashMap<Self::StorageIndex, Vec<CollisionReason>>,
        storage: &Self::Storage,
    ) -> Result<(), CollisionReason>;

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
