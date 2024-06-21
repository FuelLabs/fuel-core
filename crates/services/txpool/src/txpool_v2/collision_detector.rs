use crate::{
    txpool_v2::CoinMessage,
    types::TxId,
};
use fuel_core_types::{
    fuel_tx::UtxoId,
    fuel_types::Nonce,
    services::txpool::ArcPoolTx,
};
use std::collections::{
    HashMap,
    HashSet,
};

pub type CollidedTransactions = HashMap<TxId, Vec<CollisionReason>>;

pub struct Collision<'a> {
    new_tx: ArcPoolTx,
    collided_transactions: CollidedTransactions,
    collision_detector: &'a mut CollisionDetector,
}

impl<'a> Collision<'a> {
    pub fn apply_and_remove_collided_transactions(self) -> CollidedTransactions {
        let Collision {
            new_tx,
            collided_transactions,
            collision_detector,
        } = self;

        collision_detector.remove_from_collided_transactions(&collided_transactions);

        Self::insert(new_tx, collision_detector);
        collided_transactions
    }

    pub fn apply_and_keep_collided_transactions(self) {
        let Collision {
            new_tx,
            collided_transactions: _,
            collision_detector,
        } = self;
        Self::insert(new_tx, collision_detector);
    }

    pub fn iter_collided_transactions(&self) -> impl Iterator<Item = &ArcPoolTx> {
        let set: HashSet<TxId> = self
            .collided_transactions
            .iter()
            .map(|(tx_id, _)| *tx_id)
            .collect();

        set.into_iter().map(|tx_id| {
            self.collision_detector
                .tx_id_to_tx
                .get(&tx_id)
                .expect(
                    "`Collision` has the write ownership over `CollisionDetector`\
                    and was created from the existing `tx_id`. The corresponding `ArcPoolTx` \
                    always should be."
                )
        })
    }

    fn insert(tx: ArcPoolTx, collision_detector: &mut CollisionDetector) {
        let tx_id = tx.id();
        for collision_reasons in collision_reasons(&tx) {
            collision_detector
                .collision
                .entry(collision_reasons)
                .or_default()
                .insert(tx_id);
        }
        collision_detector.tx_id_to_tx.insert(tx_id, tx);
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum CollisionReason {
    Coin(UtxoId),
    Message(Nonce),
}

#[derive(Debug, Default)]
pub struct CollisionDetector {
    tx_id_to_tx: HashMap<TxId, ArcPoolTx>,
    collision: HashMap<CollisionReason, HashSet<TxId>>,
}

impl CollisionDetector {
    pub fn insert(&mut self, tx: ArcPoolTx) -> Option<Collision> {
        let collided_transactions = self.detect_collision(&tx);

        let collision = Collision {
            new_tx: tx,
            collided_transactions,
            collision_detector: self,
        };

        if collision.collided_transactions.is_empty() {
            collision.apply_and_keep_collided_transactions();
            None
        } else {
            Some(collision)
        }
    }

    pub fn remove(&mut self, tx_id: &TxId) {
        let tx = self.tx_id_to_tx.remove(tx_id);
        if let Some(tx) = tx {
            for collision_reason in collision_reasons(&tx) {
                if let Some(collision) = self.collision.get_mut(&collision_reason) {
                    collision.remove(tx_id);

                    if collision.is_empty() {
                        self.collision.remove(&collision_reason);
                    }
                }
            }
        }
    }

    fn remove_from_collided_transactions(
        &mut self,
        collided_transactions: &CollidedTransactions,
    ) {
        for (tx_id, collision_reasons) in collided_transactions {
            self.tx_id_to_tx.remove(tx_id);
            for collision_reason in collision_reasons {
                self.collision.remove(collision_reason);
            }
        }
    }

    fn detect_collision(&self, tx: &ArcPoolTx) -> CollidedTransactions {
        let mut collided_transactions = CollidedTransactions::new();

        for collision_reason in collision_reasons(tx) {
            if let Some(tx_ids) = self.collision.get(&collision_reason) {
                for tx_id in tx_ids {
                    let collision_reasons =
                        collided_transactions.entry(*tx_id).or_default();
                    collision_reasons.push(collision_reason);
                }
            }
        }
        collided_transactions
    }
}

pub fn collision_reasons<'a>(
    tx: &'a ArcPoolTx,
) -> impl Iterator<Item = CollisionReason> + 'a {
    tx.inputs()
        .iter()
        .filter_map(|input| match (input.coin(), input.message()) {
            (Some(utxo_id), None) => Some(CollisionReason::Coin(*utxo_id)),
            (None, Some(nonce)) => Some(CollisionReason::Message(*nonce)),
            (Some(_), Some(_)) => unreachable!(
                "The input cannot have both coin and message as the same time"
            ),
            (None, None) => None,
        })
}
