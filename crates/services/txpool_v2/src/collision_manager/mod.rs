use std::collections::HashSet;

use fuel_core_types::{
    fuel_tx::{
        BlobId,
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::PoolTransaction,
};

use crate::{
    error::Error,
    ports::TxPoolDb,
    storage::Storage,
};

pub mod basic;

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum CollisionReason {
    Coin(UtxoId),
    Blob(BlobId),
    Message(Nonce),
    ContractCreation(ContractId),
}

#[derive(Default, Debug)]
pub struct Collisions<Idx> {
    pub reasons: HashSet<CollisionReason>,
    pub colliding_txs: Vec<Idx>,
}

impl<Idx> Collisions<Idx> {
    pub fn new() -> Self {
        Self {
            reasons: HashSet::default(),
            colliding_txs: vec![],
        }
    }
}

pub trait CollisionManager<S: Storage> {
    // Error in case of our transaction is less worthy than the collided one
    fn collect_tx_collisions(
        &self,
        transaction: &PoolTransaction,
        storage: &S,
        db: &impl TxPoolDb,
    ) -> Result<Collisions<S::StorageIndex>, Error>;

    fn on_stored_transaction(
        &mut self,
        transaction: &PoolTransaction,
        transaction_id: S::StorageIndex,
    ) -> Result<(), Error>;

    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error>;
}
