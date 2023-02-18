use crate::fuel_core_graphql_api::service::Database;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
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
        TxPointer,
    },
    fuel_types::Address,
    services::txpool::TransactionStatus,
};

pub struct TransactionQueryContext<'a>(pub &'a Database);

pub trait TransactionQueryData: Send + Sync {
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction>;
    fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>>;
    fn status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus>;
    fn owned_transactions(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, Transaction)>>;
}

impl TransactionQueryData for TransactionQueryContext<'_> {
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        self.0
            .as_ref()
            .storage::<Transactions>()
            .get(tx_id)
            .and_then(|v| v.ok_or(not_found!(Transactions)).map(|tx| tx.into_owned()))
    }

    fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>> {
        self.0
            .as_ref()
            .storage::<Receipts>()
            .get(tx_id)
            .and_then(|v| v.ok_or(not_found!(Transactions)).map(|tx| tx.into_owned()))
    }

    fn status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.0.tx_status(tx_id)
    }

    fn owned_transactions(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, Transaction)>> {
        self.0
            .owned_transactions_ids(owner, start, direction)
            .map(|result| {
                result.and_then(|(tx_pointer, tx_id)| {
                    let tx = self.transaction(&tx_id)?;

                    Ok((tx_pointer, tx))
                })
            })
            .into_boxed()
    }
}

/* 
impl TransactionQueryData for Box<dyn TransactionQueryData> {
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        self.as_ref().transaction(tx_id)
    }
    fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>> {
        self.as_ref().receipts(tx_id)
    }
    fn status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.as_ref().status(tx_id)
    }
    fn owned_transactions(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, Transaction)>> {
        self.as_ref().owned_transactions(owner, start, direction)
    }
} */
