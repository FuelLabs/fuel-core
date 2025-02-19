use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::Transactions,
    Error as StorageError,
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
    services::txpool::TransactionStatusV2,
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};

impl ReadView {
    pub fn receipts(&self, tx_id: &TxId) -> StorageResult<Vec<Receipt>> {
        let status = self.tx_status(tx_id)?;

        // TODO[RC]: Should `SuccessDuringBlockProduction` and `FailureDuringBlockProduction` provide recepits?
        let receipts = match status {
            TransactionStatusV2::Success { receipts, .. }
            | TransactionStatusV2::Failure { receipts, .. } => Some(receipts),
            TransactionStatusV2::Submitted { .. }
            | TransactionStatusV2::SuccessDuringBlockProduction { .. }
            | TransactionStatusV2::SqueezedOut { .. }
            | TransactionStatusV2::SqueezedOutDuringBlockProduction { .. }
            | TransactionStatusV2::FailureDuringBlockProduction { .. } => None,
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
