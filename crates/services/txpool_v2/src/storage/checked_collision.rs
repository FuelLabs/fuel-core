use crate::error::CollisionReason;
use fuel_core_types::services::txpool::PoolTransaction;
use std::collections::{
    HashMap,
    HashSet,
};

pub struct CheckedTransaction<StorageIndex> {
    tx: PoolTransaction,
    all_dependencies: HashSet<StorageIndex>,
}

impl<StorageIndex> CheckedTransaction<StorageIndex> {
    /// Creates a new checked transactions.
    ///
    /// It is a private method called by the `GraphStorage`.
    pub(super) fn new(
        tx: PoolTransaction,
        all_dependencies: HashSet<StorageIndex>,
    ) -> Self {
        Self {
            tx,
            all_dependencies,
        }
    }
}

impl<StorageIndex> super::CheckedTransaction<StorageIndex>
    for CheckedTransaction<StorageIndex>
{
    fn tx(&self) -> &PoolTransaction {
        &self.tx
    }

    fn into_tx(self) -> PoolTransaction {
        self.tx
    }

    fn all_dependencies(&self) -> &HashSet<StorageIndex> {
        &self.all_dependencies
    }
}
