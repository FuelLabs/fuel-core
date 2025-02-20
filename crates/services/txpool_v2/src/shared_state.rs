use std::sync::Arc;

use anyhow::anyhow;
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::txpool::{
        ArcPoolTx,
        TransactionStatus,
    },
};
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot::{
        self,
        error::TryRecvError,
    },
    watch,
};

use crate::{
    error::Error,
    pool::TxPoolStats,
    pool_worker,
    pool_worker::{
        PoolReadRequest,
        PoolRemoveRequest,
    },
    service::{
        TxInfo,
        WritePoolRequest,
    },
    tx_status_stream::{
        TxStatusMessage,
        TxStatusStream,
    },
    update_sender::{
        MpscChannel,
        TxStatusChange,
    },
    Constraints,
};

#[derive(Clone)]
pub struct SharedState {
    pub(crate) request_remove_sender: mpsc::Sender<PoolRemoveRequest>,
    pub(crate) write_pool_requests_sender: mpsc::Sender<WritePoolRequest>,
    pub(crate) select_transactions_requests_sender:
        mpsc::Sender<pool_worker::PoolExtractBlockTransactions>,
    pub(crate) request_read_sender: mpsc::Sender<PoolReadRequest>,
    pub(crate) tx_status_sender: TxStatusChange,
    pub(crate) new_txs_notifier: tokio::sync::watch::Sender<()>,
    pub(crate) latest_stats: tokio::sync::watch::Receiver<TxPoolStats>,
}

impl SharedState {
    pub fn try_insert(&self, transactions: Vec<Transaction>) -> Result<(), Error> {
        let transactions = transactions.into_iter().map(Arc::new).collect();
        self.write_pool_requests_sender
            .try_send(WritePoolRequest::InsertTxs { transactions })
            .map_err(|_| Error::ServiceQueueFull)?;

        Ok(())
    }

    pub async fn insert(&self, transaction: Transaction) -> Result<(), Error> {
        let transaction = Arc::new(transaction);
        let (sender, receiver) = oneshot::channel();

        self.write_pool_requests_sender
            .send(WritePoolRequest::InsertTx {
                transaction,
                response_channel: sender,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?
    }

    /// This function has a hot loop inside to acquire transactions for the execution.
    /// It relies on the prioritization of the `TxPool`
    /// (it always tries to prioritize the `extract` call over other calls).
    /// In the future, extraction will be an async function,
    /// and we can remove this loop and just `await`.
    pub fn extract_transactions_for_block(
        &self,
        constraints: Constraints,
    ) -> Result<Vec<ArcPoolTx>, Error> {
        let (select_transactions_sender, mut select_transactions_receiver) =
            oneshot::channel();
        self.select_transactions_requests_sender
            .try_send(
                pool_worker::PoolExtractBlockTransactions::ExtractBlockTransactions {
                    constraints,
                    transactions: select_transactions_sender,
                },
            )
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        loop {
            let result = select_transactions_receiver.try_recv();
            match result {
                Ok(txs) => {
                    return Ok(txs);
                }
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Closed) => {
                    return Err(Error::ServiceCommunicationFailed);
                }
            }
        }
    }

    pub async fn get_tx_ids(&self, max_txs: usize) -> Result<Vec<TxId>, Error> {
        let (response_channel, result_receiver) = oneshot::channel();

        self.request_read_sender
            .send(PoolReadRequest::TxIds {
                max_txs,
                response_channel,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        result_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    pub async fn find_one(&self, tx_id: TxId) -> Result<Option<TxInfo>, Error> {
        Ok(self.find(vec![tx_id]).await?.pop().flatten())
    }

    pub async fn find(&self, tx_ids: Vec<TxId>) -> Result<Vec<Option<TxInfo>>, Error> {
        let (response_channel, result_receiver) = oneshot::channel();

        self.request_read_sender
            .send(PoolReadRequest::Txs {
                tx_ids,
                response_channel,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        result_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    /// Get a notifier that is notified when new transactions are added to the pool.
    pub fn get_new_txs_notifier(&self) -> watch::Receiver<()> {
        self.new_txs_notifier.subscribe()
    }

    /// Subscribe to new transaction notifications.
    pub fn new_tx_notification_subscribe(&self) -> broadcast::Receiver<TxId> {
        self.tx_status_sender.new_tx_notification_sender.subscribe()
    }

    /// Subscribe to status updates for a transaction.
    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_sender
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }

    /// Notify the txpool that a transaction was executed and committed to a block.
    pub fn notify_complete_tx(
        &self,
        id: Bytes32,
        block_height: &BlockHeight,
        status: TransactionStatus,
    ) {
        self.tx_status_sender.send_complete(
            id,
            block_height,
            TxStatusMessage::Status(status),
        )
    }

    /// Notify the txpool that some transactions were skipped during block production.
    /// This is used to update the status of the skipped transactions internally and in subscriptions
    pub fn notify_skipped_txs(&self, tx_ids_and_reason: Vec<(Bytes32, String)>) {
        let dependents_ids = tx_ids_and_reason
            .into_iter()
            .map(|(tx_id, reason)| {
                let error = Error::SkippedTransaction(reason);
                self.tx_status_sender
                    .send_squeezed_out(tx_id, error.clone());
                (tx_id, error)
            })
            .collect();

        if let Err(e) = self
            .request_remove_sender
            .try_send(PoolRemoveRequest::CoinDependents { dependents_ids })
        {
            tracing::error!("Failed to send remove coin dependents request: {}", e);
        }
    }

    pub fn latest_stats(&self) -> TxPoolStats {
        *self.latest_stats.borrow()
    }
}
