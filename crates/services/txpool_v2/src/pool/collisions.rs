use crate::{
    collision_manager::Collisions,
    error::CollisionReason,
    storage::Storage,
};
use fuel_core_types::services::txpool::PoolTransaction;
use num_rational::Ratio;

/// Trait that extends the `Collision` type functionality used by the pool.
pub trait CollisionsExt<S> {
    /// Determine if the collisions allow the transaction to be stored.
    /// Returns the reason of the collision if the transaction cannot be stored.
    fn check_collision_requirements(
        &self,
        tx: &PoolTransaction,
        has_dependencies: bool,
        storage: &S,
    ) -> Result<(), CollisionReason>;
}

impl<S> CollisionsExt<S> for Collisions<S::StorageIndex>
where
    S: Storage,
{
    /// Rules:
    /// - A transaction has dependencies:
    ///     - Can collide only with one other transaction. So, the user can submit
    ///         the same transaction with a higher tip but not merge one or more
    ///         transactions into one.
    ///     - A new transaction can be accepted if its profitability is higher than
    ///         the collided subtree's.
    /// - A transaction doesn't have dependencies:
    ///     - A new transaction can be accepted if its profitability is higher
    ///         than the collided subtrees'.
    fn check_collision_requirements(
        &self,
        tx: &PoolTransaction,
        has_dependencies: bool,
        storage: &S,
    ) -> Result<(), CollisionReason> {
        if has_dependencies && self.len() > 1 {
            return Err(CollisionReason::MultipleCollisions);
        }

        for (collision, reason) in self.iter() {
            if !is_better_than_collision(tx, collision, storage)? {
                if let Some(reason) = reason.first() {
                    return Err(reason.clone());
                } else {
                    return Err(CollisionReason::Unknown);
                }
            }
        }

        Ok(())
    }
}

fn is_better_than_collision<S>(
    tx: &PoolTransaction,
    collision: &S::StorageIndex,
    storage: &S,
) -> Result<bool, CollisionReason>
where
    S: Storage,
{
    let new_tx_ratio = Ratio::new(tx.tip(), tx.max_gas());
    let colliding_tx = storage.get(collision).ok_or(CollisionReason::Unknown)?;
    let colliding_tx_ratio = Ratio::new(
        colliding_tx.dependents_cumulative_tip,
        colliding_tx.dependents_cumulative_gas,
    );
    Ok(new_tx_ratio > colliding_tx_ratio)
}
