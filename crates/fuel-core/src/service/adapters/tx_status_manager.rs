use fuel_core_txpool::ports::TxStatusManager;
use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::TransactionStatus,
};

use super::TxStatusManagerAdapter;

impl TxStatusManager for TxStatusManagerAdapter {
    fn add_status(&self, tx_id: &TxId, tx_status: &TransactionStatus) -> u32 {
        self.service.add_status(tx_id, tx_status)
    }
}
