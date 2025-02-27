use fuel_core_txpool::ports::TxStatusManager;
use fuel_core_types::{
    fuel_asm::RegId,
    fuel_tx::{
        TransactionBuilder,
        TxId,
    },
    services::txpool::TransactionStatus,
};

use crate::service::FuelService;

use super::TxStatusManagerAdapter;

impl TxStatusManager for TxStatusManagerAdapter {
    fn add_status(&mut self, tx_id: &TxId, tx_status: &TransactionStatus) {
        self.service.add_status(tx_id, tx_status)
    }

    fn status(&self, tx_id: &TxId) -> Option<&TransactionStatus> {
        self.service.status(tx_id)
    }
}
