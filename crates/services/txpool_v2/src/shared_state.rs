use std::sync::Arc;

use anyhow::anyhow;
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::txpool::TransactionStatus,
};
use parking_lot::RwLockWriteGuard;
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
    watch,
};

use crate::{
    error::Error,
    pool::TxPoolStats,
    service::{
        BorrowTxPoolRequest,
        ReadPoolRequest,
        Shared,
        TxInfo,
        TxPool,
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
};

pub struct BorrowedTxPool(pub(crate) Shared<TxPool>);

impl BorrowedTxPool {
    /// Get a write lock on the TxPool.
    pub fn exclusive_lock(&self) -> RwLockWriteGuard<TxPool> {
        self.0.write()
    }
}

#[derive(Clone)]
pub struct SharedState {
    pub(crate) write_pool_requests_sender: mpsc::Sender<WritePoolRequest>,
    pub(crate) select_transactions_requests_sender: mpsc::Sender<BorrowTxPoolRequest>,
    pub(crate) read_pool_requests_sender: mpsc::Sender<ReadPoolRequest>,
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

    pub async fn borrow_txpool(&self) -> Result<BorrowedTxPool, Error> {
        let (select_transactions_sender, select_transactions_receiver) =
            oneshot::channel();
        self.select_transactions_requests_sender
            .send(BorrowTxPoolRequest {
                response_channel: select_transactions_sender,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;

        select_transactions_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)
    }

    pub async fn get_tx_ids(&self, max_txs: usize) -> Result<Vec<TxId>, Error> {
        let (result_sender, result_receiver) = oneshot::channel();
        self.read_pool_requests_sender
            .send(ReadPoolRequest::GetTxIds {
                max_txs,
                response_channel: result_sender,
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
        let (result_sender, result_receiver) = oneshot::channel();
        self.read_pool_requests_sender
            .send(ReadPoolRequest::GetTxs {
                tx_ids,
                response_channel: result_sender,
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
        let transactions = tx_ids_and_reason
            .into_iter()
            .map(|(tx_id, reason)| {
                self.tx_status_sender
                    .send_squeezed_out(tx_id, Error::SkippedTransaction(reason.clone()));
                (tx_id, reason)
            })
            .collect();

        let _ = self
            .write_pool_requests_sender
            .try_send(WritePoolRequest::RemoveCoinDependents { transactions });
    }

    pub fn latest_stats(&self) -> TxPoolStats {
        *self.latest_stats.borrow()
    }
}
