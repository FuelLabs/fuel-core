use std::collections::HashMap;

use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::TransactionStatus,
};

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

    pub fn add_status(&mut self, tx_id: &TxId, tx_status: TransactionStatus) {
        self.statuses.insert(tx_id.clone(), tx_status);
    }

    pub fn status(&self, tx_id: &TxId) -> Option<&TransactionStatus> {
        self.statuses.get(tx_id)
    }
}
