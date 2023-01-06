use crate::fuel_core_graphql_api::service::DatabaseTemp;
use fuel_core_storage::{
    not_found,
    tables::{
        Receipts,
        Transactions,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    fuel_tx::{
        Receipt,
        Transaction,
    },
    services::txpool::TransactionStatus,
};

pub struct TransactionQueryContext<'a>(pub &'a DatabaseTemp);

impl TransactionQueryContext<'_> {
    pub fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        self.0
            .as_ref()
            .storage::<Transactions>()
            .get(tx_id)
            .and_then(|v| v.ok_or(not_found!(Transactions)).map(|tx| tx.into_owned()))
    }

    pub fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>> {
        self.0
            .as_ref()
            .storage::<Receipts>()
            .get(tx_id)
            .and_then(|v| v.ok_or(not_found!(Transactions)).map(|tx| tx.into_owned()))
    }

    pub fn status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.0.tx_status(tx_id)
    }
}
