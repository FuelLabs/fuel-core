use std::collections::HashMap;

use fuel_core_types::{
    fuel_tx::TxId,
    services::{
        p2p::TransactionStatusGossipData,
        txpool::TransactionStatus,
    },
};
use tokio::sync::broadcast;

const CHANNEL_SIZE: usize = 1024 * 10;

#[derive(Clone)]
pub struct SharedState {
    statuses: HashMap<TxId, TransactionStatus>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            statuses: HashMap::new(),
        }
    }

    pub fn upsert_status(&mut self, tx_id: &TxId, tx_status: TransactionStatus) {
        // TODO[RC]: Take care about locking here
        self.statuses.insert(tx_id.clone(), tx_status);
    }

    pub fn status(&self, tx_id: &TxId) -> Option<&TransactionStatus> {
        self.statuses.get(tx_id)
    }
}
