use fuel_core_types::services::txpool::PoolTransaction;

use crate::storage::StorageData;

pub mod basic;
pub mod collision;

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;
    /// The collision type returned by the collision manager.
    type Collision;

    /// Finds collision for the transaction. Returns `Self::Collision`, if find any.
    fn find_collision(&self, transaction: PoolTransaction) -> Self::Collision;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);
}
