use std::{
    collections::HashSet,
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

use crate::error::Error;

pub mod basic;

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;

    /// Collect the transaction that collide with the given transaction.
    /// It returns an error if the transaction is less worthy than the colliding transactions.
    /// It returns an error if the transaction collide with two transactions.
    /// It returns the information about the collision if exists, none otherwise.
    fn collect_colliding_transaction(
        &self,
        transaction: &PoolTransaction,
        storage: &Self::Storage,
    ) -> Result<Option<Self::StorageIndex>, Error>;

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
