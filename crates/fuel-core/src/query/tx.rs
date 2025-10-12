use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    Error as StorageError,
    Result as StorageResult,
    iter::IterDirection,
    not_found,
    tables::Transactions,
};
use fuel_core_types::{
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
        TxPointer,
    },
    fuel_types::Address,
    services::transaction_status::TransactionExecutionStatus,
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use std::sync::Arc;

impl ReadView {
    pub fn receipts(&self, tx_id: &TxId) -> StorageResult<Arc<Vec<Receipt>>> {
        let status = self.tx_status(tx_id)?;

        let receipts = match status {
            TransactionExecutionStatus::Success { receipts, .. }
            | TransactionExecutionStatus::Failed { receipts, .. } => Some(receipts),
            TransactionExecutionStatus::Submitted { .. }
            | TransactionExecutionStatus::SqueezedOut { .. } => None,
        };

        receipts.ok_or(not_found!(Transactions))
    }

    pub fn owned_transactions(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<(TxPointer, Transaction)>> + '_ {
        self.owned_transactions_ids(owner, start, direction)
            .chunks(self.batch_size)
            .map(|chunk| {
                use itertools::Itertools;

                let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                Ok::<_, StorageError>(chunk)
            })
            .try_filter_map(move |chunk| async move {
                let tx_ids = chunk.iter().map(|(_, tx_id)| *tx_id).collect::<Vec<_>>();
                let txs = self.transactions(tx_ids).await;
                let txs = txs
                    .into_iter()
                    .zip(chunk)
                    .map(|(result, (tx_pointer, _))| result.map(|tx| (tx_pointer, tx)));
                Ok(Some(futures::stream::iter(txs)))
            })
            .try_flatten()
    }
}
