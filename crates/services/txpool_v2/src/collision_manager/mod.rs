use fuel_core_types::services::txpool::PoolTransaction;

use crate::storage::StorageData;

pub mod basic;
pub mod collisions;

pub trait CollisionManager {
    /// Storage type of the collision manager.
    type Storage;
    /// Index that identifies a transaction in the storage.
    type StorageIndex;
    /// The type to represent collisions of the transaction.
    type Collisions;

    /// Finds collisions for the transaction.
    fn find_collisions(&self, transaction: PoolTransaction) -> Self::Collisions;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);
}
