use crate::error::{
    CollisionReason,
    Error,
};
use fuel_core_types::services::txpool::PoolTransaction;
use std::collections::HashMap;

use crate::storage::StorageData;

pub mod basic;

pub type Collisions<StorageIndex> = HashMap<StorageIndex, Vec<CollisionReason>>;

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;

    /// Finds collisions for the transaction.
    fn find_collisions(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<Collisions<Self::StorageIndex>, Error>;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);
}
