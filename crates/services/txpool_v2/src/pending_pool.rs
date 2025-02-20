#![allow(dead_code)]
use std::time::Duration;

use fuel_core_types::{
    fuel_tx::{
        ContractId,
        UtxoId,
    },
    fuel_types::Nonce,
    services::txpool::ArcPoolTx,
};

// This is a simple temporary storage for transactions that doesn't have all of their input created yet.
// This storage should not have a lot of complexity.
//
// Insertion rules:
// - If the transaction has one or more inputs that are not known yet.
//
// Deletion rules:
// - If the transaction missing inputs hasn't been known for a certain amount of time.
// - If the transaction missing inputs becomes known.
pub struct PendingPool {
    ttl: Duration,
}

pub enum MissingInput {
    Utxo(UtxoId),
    Contract(ContractId),
    Message(Nonce),
}

pub struct UpdateMissingTxs {
    pub resolved_txs: Vec<ArcPoolTx>,
    pub expired_txs: Vec<ArcPoolTx>,
}

impl PendingPool {
    // Review question: Should we add size ?
    pub fn new(ttl: Duration) -> Self {
        Self { ttl }
    }

    // Review question: When we are missing an input in `validate_inputs` we return directly an error.
    // It means that it can be possible that we are waiting for 2 UTXOs and we will only get one.
    // Should we change the behavior of `validate_inputs` to return all the missing inputs ?
    pub fn insert_transaction(
        &mut self,
        _transaction: ArcPoolTx,
        _missing_input: MissingInput,
    ) {
        todo!()
    }

    // Review question: Is it ok to check ttl of pending transactions in this function ?
    // We expect it to be called a lot (every new block, every new tx in the pool).
    pub fn new_known_txs(&mut self, _new_known_txs: Vec<ArcPoolTx>) -> UpdateMissingTxs {
        todo!()
    }
}
