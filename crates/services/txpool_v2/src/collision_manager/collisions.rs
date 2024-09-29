use crate::{
    error::CollisionReason,
    storage,
};
use fuel_core_types::services::txpool::PoolTransaction;
use std::collections::HashMap;

pub struct SimpleCollisions<StorageIndex> {
    tx: PoolTransaction,
    /// Colliding transactions.
    transactions: HashMap<StorageIndex, Vec<CollisionReason>>,
}

impl<StorageIndex> SimpleCollisions<StorageIndex> {
    /// Creates a new collision from its colliding transactions.
    pub(super) fn new(
        tx: PoolTransaction,
        transactions: HashMap<StorageIndex, Vec<CollisionReason>>,
    ) -> Self {
        Self { tx, transactions }
    }
}

impl<StorageIndex> storage::TransactionWithCollisions<StorageIndex>
    for SimpleCollisions<StorageIndex>
{
    fn tx(&self) -> &PoolTransaction {
        &self.tx
    }

    fn colliding_transactions(&self) -> &HashMap<StorageIndex, Vec<CollisionReason>> {
        &self.transactions
    }

    fn into_instigator(self) -> PoolTransaction {
        self.tx
    }
}
