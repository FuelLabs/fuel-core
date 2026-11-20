use crate::error::{
    CollisionReason,
    Error,
};
use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::PoolTransaction,
};
use std::collections::HashMap;

use fuel_core_types::fuel_tx::ContractId;

use crate::storage::StorageData;

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

    /// Get the IDs of pool transactions that have `Input::Contract(contract_id)`.
    /// Used during preconfirmation rollback to evict pool txs that were admitted
    /// only because a preconfirmed tx temporarily created the contract.
    fn get_contract_users(&self, contract_id: &ContractId) -> Vec<TxId>;

    /// Returns true if a currently in-pool transaction creates `contract_id`.
    /// Used during rollback to skip eviction when the contract is still available
    /// via a pool tx (independent of the rolled-back preconfirmation).
    fn contract_created_in_pool(&self, contract_id: &ContractId) -> bool;

    /// Inform the collision manager that a transaction was stored.
    fn on_stored_transaction(
        &mut self,
        transaction_id: Self::StorageIndex,
        store_entry: &StorageData,
    );

    /// Inform the collision manager that a transaction was removed from the pool
    /// for any reason other than canonical-block commitment (e.g. eviction, TTL
    /// expiry, rollback-driven eviction).  Cleans up all tracking state including
    /// the `contract_users` entry for this transaction.
    fn on_removed_transaction(&mut self, transaction: &PoolTransaction);

    /// Inform the collision manager that a transaction was committed to a
    /// canonical block and has been removed from the pool.  Like
    /// `on_removed_transaction` but intentionally **skips** the removal of
    /// this transaction's ID from `contract_users`.  The caller is responsible
    /// for invoking `remove_from_contract_users` later — after any
    /// preconfirmation-rollback work is complete — so that rollback logic can
    /// still observe the full set of contract dependents.
    fn on_committed_transaction(&mut self, transaction: &PoolTransaction);

    /// Remove the transaction's ID from every `contract_users` entry it
    /// contributed to.  Call this once per committed transaction, after all
    /// rollback logic for the relevant block height has finished.
    fn remove_from_contract_users(&mut self, transaction: &PoolTransaction);
}
