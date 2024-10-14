use crate::fuel_core_graphql_api::database::ReadView;

use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    tables::Transactions,
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
        TxPointer,
    },
    fuel_types::Address,
    services::txpool::TransactionStatus,
};

impl ReadView {
    pub fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>> {
        let status = self.status(tx_id)?;

        let receipts = match status {
            TransactionStatus::Success { receipts, .. }
            | TransactionStatus::Failed { receipts, .. } => Some(receipts),
            _ => None,
        };
        receipts.ok_or(not_found!(Transactions))
    }

    pub fn status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.tx_status(tx_id)
    }

    pub fn owned_transactions(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, Transaction)>> {
        self.owned_transactions_ids(owner, start, direction)
            .map(|result| {
                result.and_then(|(tx_pointer, tx_id)| {
                    let tx = self.transaction(&tx_id)?;

                    Ok((tx_pointer, tx))
                })
            })
            .into_boxed()
    }
}
