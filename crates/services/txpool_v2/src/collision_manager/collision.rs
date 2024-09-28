use crate::{
    collision_manager::{
        basic::BasicCollisionManagerStorage,
        Collision,
    },
    error::CollisionReason,
};
use fuel_core_types::services::txpool::PoolTransaction;
use num_rational::Ratio;
use std::collections::HashMap;

pub struct SimpleCollision<'a, StorageIndex> {
    tx: &'a PoolTransaction,
    /// Colliding transactions.
    transactions: HashMap<StorageIndex, Vec<CollisionReason>>,
}

impl<'a, StorageIndex> SimpleCollision<'a, StorageIndex> {
    /// Creates a new collision from its colliding transactions.
    pub(super) fn new(
        tx: &'a PoolTransaction,
        transactions: HashMap<StorageIndex, Vec<CollisionReason>>,
    ) -> Self {
        Self { tx, transactions }
    }
}

impl<'a, StorageIndex> Collision<StorageIndex> for SimpleCollision<'a, StorageIndex> {
    fn tx(&self) -> &PoolTransaction {
        self.tx
    }

    fn colliding_transactions(&self) -> &HashMap<StorageIndex, Vec<CollisionReason>> {
        &self.transactions
    }

    fn into_colliding_storage(self) -> Vec<StorageIndex> {
        self.transactions.into_keys().collect()
    }
}
