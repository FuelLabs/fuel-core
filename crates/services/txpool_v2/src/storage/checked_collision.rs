use crate::error::CollisionReason;
use fuel_core_types::services::txpool::PoolTransaction;
use std::collections::{
    HashMap,
    HashSet,
};

pub struct CheckedCollision<Collision, StorageIndex> {
    collision: Collision,
    all_dependencies: HashSet<StorageIndex>,
}

impl<Collision, StorageIndex> CheckedCollision<Collision, StorageIndex> {
    /// Creates a new checked collision.
    pub(super) fn new(
        collision: Collision,
        all_dependencies: HashSet<StorageIndex>,
    ) -> Self {
        Self {
            collision,
            all_dependencies,
        }
    }
}

impl<StorageIndex, Collision> super::Collision<StorageIndex>
    for CheckedCollision<Collision, StorageIndex>
where
    Collision: super::Collision<StorageIndex>,
{
    fn tx(&self) -> &PoolTransaction {
        self.collision.tx()
    }

    fn colliding_transactions(&self) -> &HashMap<StorageIndex, Vec<CollisionReason>> {
        self.collision.colliding_transactions()
    }

    fn into_instigator(self) -> PoolTransaction {
        self.collision.into_instigator()
    }
}

impl<StorageIndex, Collision> super::CheckedCollision<StorageIndex>
    for CheckedCollision<Collision, StorageIndex>
where
    Collision: super::Collision<StorageIndex>,
{
    fn all_dependencies(&self) -> &HashSet<StorageIndex> {
        &self.all_dependencies
    }
}
