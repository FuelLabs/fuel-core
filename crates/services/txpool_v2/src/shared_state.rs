use std::sync::Arc;

use anyhow::anyhow;
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::{
        p2p::GossipsubMessageInfo,
        txpool::{
            ArcPoolTx,
            TransactionStatus,
        },
    },
};
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
    Notify,
};

use crate::{
    error::Error,
    selection_algorithms::Constraints,
    service::{
        InsertionRequest,
        ReadPoolRequest,
        SelectTransactionsRequest,
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

pub struct SharedState {
    pub(crate) txs_insert_sender: mpsc::Sender<InsertionRequest>,
    pub(crate) select_transactions_requests_sender:
        mpsc::Sender<SelectTransactionsRequest>,
    pub(crate) read_pool_requests_sender: mpsc::Sender<ReadPoolRequest>,
    pub(crate) tx_status_sender: TxStatusChange,
    pub(crate) new_txs_notifier: Arc<Notify>,
}

impl Clone for SharedState {
    fn clone(&self) -> Self {
        SharedState {
            new_txs_notifier: self.new_txs_notifier.clone(),
            select_transactions_requests_sender: self
                .select_transactions_requests_sender
                .clone(),
            txs_insert_sender: self.txs_insert_sender.clone(),
            tx_status_sender: self.tx_status_sender.clone(),
            read_pool_requests_sender: self.read_pool_requests_sender.clone(),
        }
    }
}

impl SharedState {
    pub async fn insert(
        &self,
        transactions: Vec<Arc<Transaction>>,
        from_peer_info: Option<GossipsubMessageInfo>,
    ) -> Result<(), Error> {
        let (txs_insert_sender, txs_insert_receiver) = oneshot::channel();
        self.txs_insert_sender
            .send(InsertionRequest {
                transactions,
                from_peer_info,
                response_channel: Some(txs_insert_sender),
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;
        txs_insert_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?
    }

    pub async fn select_transactions(
        &self,
        max_gas: u64,
    ) -> Result<Vec<ArcPoolTx>, Error> {
        let (select_transactions_sender, select_transactions_receiver) =
            oneshot::channel();
        self.select_transactions_requests_sender
            .send(SelectTransactionsRequest {
                constraints: Constraints { max_gas },
                response_channel: select_transactions_sender,
            })
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?;
        select_transactions_receiver
            .await
            .map_err(|_| Error::ServiceCommunicationFailed)?
    }

    pub fn get_new_txs_notifier(&self) -> Arc<Notify> {
        self.new_txs_notifier.clone()
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

    pub async fn find_one(&self, tx_id: TxId) -> Result<Option<ArcPoolTx>, Error> {
        Ok(self.find(vec![tx_id]).await?.pop().flatten())
    }

    pub async fn find(&self, tx_ids: Vec<TxId>) -> Result<Vec<Option<ArcPoolTx>>, Error> {
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

    pub fn new_tx_notification_subscribe(&self) -> broadcast::Receiver<TxId> {
        self.tx_status_sender.new_tx_notification_sender.subscribe()
    }

    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_sender
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }

    pub fn send_complete(
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

    pub fn broadcast_txs_skipped_reason(
        &self,
        tx_ids_and_reason: Vec<(Bytes32, String)>,
    ) {
        for (tx_id, reason) in tx_ids_and_reason {
            self.tx_status_sender
                .send_squeezed_out(tx_id, Error::SkippedTransaction(reason));
        }
    }
}
