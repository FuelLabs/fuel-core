use crate::error::{
    CollisionReason,
    Error,
};
use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::PoolTransaction,
};
use std::collections::HashMap;

use crate::storage::StorageData;

#[cfg(test)]
use fuel_core_types::services::txpool::ArcPoolTx;

pub mod basic;

pub type Collisions<StorageIndex> = HashMap<StorageIndex, Vec<CollisionReason>>;

pub trait CollisionManager {
    /// Index that identifies a transaction in the storage.
    type StorageIndex;

    /// Finds collisions for the transaction.
    fn find_collisions(
        &self,
        transaction: &PoolTransaction,
    ) -> Result<Collisions<Self::StorageIndex>, Error>;

    /// Get spenders of coins UTXO created by a transaction ID.
    fn get_coins_spenders(&self, tx_creator_id: &TxId) -> Vec<Self::StorageIndex>;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);

    #[cfg(test)]
    /// Asserts the integrity of the collision manager.
    fn assert_integrity(&self, expected_txs: &[ArcPoolTx]);
}
