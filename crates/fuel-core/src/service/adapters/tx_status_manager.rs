use fuel_core_txpool::ports::TxStatusManager;

use super::TxStatusManagerAdapter;

impl TxStatusManager for TxStatusManagerAdapter {
    fn foo(&self) -> u32 {
        12398
    }
}
