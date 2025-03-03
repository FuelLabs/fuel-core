use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::TransactionStatus,
};

#[derive(Clone)]
pub struct TxStatusManager {
    statuses: Arc<Mutex<HashMap<TxId, TransactionStatus>>>,
}

impl TxStatusManager {
    pub fn new() -> Self {
        Self {
            statuses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn upsert_status(&self, tx_id: &TxId, tx_status: TransactionStatus) {
        // TODO[RC]: Capacity checks?
        // TODO[RC]: Purge old statuses?
        // TODO[RC]: Remember to remove the status from the manager upon putting the status into storage.

        self.statuses
            .lock()
            .expect("mutex poisoned")
            .insert(tx_id.clone(), tx_status);
    }

    pub fn status(&self, tx_id: &TxId) -> Option<TransactionStatus> {
        self.statuses
            .lock()
            .expect("mutex poisoned")
            .get(tx_id)
            .map(|s| s.clone())
    }
}
