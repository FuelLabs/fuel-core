use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::{
        p2p::GossipsubMessageInfo,
        txpool::ArcPoolTx,
    },
};
use std::{
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::{
    mpsc,
    mpsc::{
        UnboundedReceiver,
        UnboundedSender,
    },
    oneshot,
};

use crate::{
    error::{
        Error,
        RemovedReason,
    },
    ports::TxPoolPersistentStorage,
    service::{
        TxInfo,
        TxPool,
    },
    Constraints,
};

pub struct PoolWorkerInterface {
    pub(crate) thread_management_sender: UnboundedSender<ThreadManagementRequest>,
    pub(crate) request_insert_sender: UnboundedSender<PoolInsertRequest>,
    pub(crate) request_remove_sender: UnboundedSender<PoolRemoveRequest>,
    pub(crate) request_read_sender: UnboundedSender<PoolReadRequest>,
    pub(crate) extract_block_transactions_sender:
        UnboundedSender<PoolExtractBlockTransactions>,
    pub(crate) notification_receiver: UnboundedReceiver<PoolNotification>,
    pub(crate) handle: Option<std::thread::JoinHandle<()>>,
}

impl PoolWorkerInterface {
    pub fn new<View>(
        tx_pool: TxPool,
        view_provider: Arc<dyn AtomicView<LatestView = View>>,
    ) -> Self
    where
        View: TxPoolPersistentStorage,
    {
        let (request_read_sender, request_read_receiver) = mpsc::unbounded_channel();
        let (extract_block_transactions_sender, extract_block_transactions_receiver) =
            mpsc::unbounded_channel();
        let (request_remove_sender, request_remove_receiver) = mpsc::unbounded_channel();
        let (request_insert_sender, request_insert_receiver) = mpsc::unbounded_channel();
        let (notification_sender, notification_receiver) = mpsc::unbounded_channel();
        let (thread_management_sender, thread_management_receiver) =
            mpsc::unbounded_channel();

        let handle = std::thread::spawn(move || {
            let tokio_runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let mut worker = PoolWorker {
                thread_management_receiver,
                request_remove_receiver,
                request_read_receiver,
                extract_block_transactions_receiver,
                request_insert_receiver,
                notification_sender,
                pool: tx_pool,
                view_provider,
            };

            tokio_runtime.block_on(async { while !worker.run().await {} });
        });
        Self {
            thread_management_sender,
            request_insert_sender,
            extract_block_transactions_sender,
            request_read_sender,
            request_remove_sender,
            notification_receiver,
            handle: Some(handle),
        }
    }

    pub fn remove(&self, tx_ids: Vec<TxId>) {
        if let Err(e) = self
            .request_remove_sender
            .send(PoolRemoveRequest::Remove { tx_ids })
        {
            tracing::error!("Failed to send remove request: {}", e);
        }
    }

    pub fn remove_coin_dependents(&self, parent_txs: Vec<(TxId, String)>) {
        if let Err(e) = self
            .request_remove_sender
            .send(PoolRemoveRequest::RemoveCoinDependents { parent_txs })
        {
            tracing::error!("Failed to send remove coin dependents request: {}", e);
        }
    }

    pub fn remove_and_coin_dependents(&self, tx_ids: (Vec<TxId>, Error)) {
        if let Err(e) = self
            .request_remove_sender
            .send(PoolRemoveRequest::RemoveAndCoinDependents { tx_ids })
        {
            tracing::error!("Failed to send remove and coin dependents request: {}", e);
        }
    }

    pub fn get_tx_ids(
        &self,
        max_txs: usize,
        response_channel: oneshot::Sender<Vec<TxId>>,
    ) {
        if let Err(e) = self.request_read_sender.send(PoolReadRequest::TxIds {
            max_txs,
            tx_ids: response_channel,
        }) {
            tracing::error!("Failed to send tx ids request: {}", e);
        }
    }

    pub fn get_txs(&self, tx_ids: Vec<TxId>, txs: oneshot::Sender<Vec<Option<TxInfo>>>) {
        if let Err(e) = self
            .request_read_sender
            .send(PoolReadRequest::Txs { tx_ids, txs })
        {
            tracing::error!("Failed to send get txs request: {}", e);
        }
    }

    pub fn stop(&mut self) {
        if let Err(e) = self
            .thread_management_sender
            .send(ThreadManagementRequest::Stop)
        {
            tracing::error!("Failed to send stop request: {}", e);
        }
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                tracing::error!("Failed to join pool worker thread");
            }
        }
    }
}

pub enum ThreadManagementRequest {
    Stop,
}

// In the real code we want to have more different channels to prioritise each type of query
// We are only prioritising this one for bench for now
// The order would probably be :
// 1. Getting txs for a block
// 2. Inserting txs
// 3. Removing txs / removing coins depedents ...
// 4. Read API/P2P...
pub enum PoolInsertRequest {
    Insert {
        tx: ArcPoolTx,
        tx_for_p2p: Arc<Transaction>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    },
}

pub enum PoolExtractBlockTransactions {
    ExtractBlockTransactions {
        constraints: Constraints,
        transactions: oneshot::Sender<Vec<ArcPoolTx>>,
    },
}

pub enum PoolRemoveRequest {
    Remove { tx_ids: Vec<TxId> },
    // TODO: It duplicates `WritePoolRequest::RemoveCoinDependents`. We need to work directly with
    //  `RemoveCoinDependents` without `WritePoolRequest::RemoveCoinDependents` at the middle.
    RemoveCoinDependents { parent_txs: Vec<(TxId, String)> },
    RemoveAndCoinDependents { tx_ids: (Vec<TxId>, Error) },
}
pub enum PoolReadRequest {
    NonExistingTxs {
        tx_ids: Vec<TxId>,
        non_existing_txs: oneshot::Sender<Vec<TxId>>,
    },
    // TODO: Avoid duplication of the `ReadPoolRequest::GetTxs`
    Txs {
        tx_ids: Vec<TxId>,
        txs: oneshot::Sender<Vec<Option<TxInfo>>>,
    },
    // TODO: Avoid duplication of the `ReadPoolRequest::GetTxIds`
    TxIds {
        max_txs: usize,
        tx_ids: oneshot::Sender<Vec<TxId>>,
    },
}

pub enum PoolNotification {
    Inserted {
        tx_id: TxId,
        time: SystemTime,
        expiration: BlockHeight,
        from_peer_info: Option<GossipsubMessageInfo>,
        tx: Arc<Transaction>,
    },
    ErrorInsertion {
        tx_id: TxId,
        error: Error,
        from_peer_info: Option<GossipsubMessageInfo>,
    },
    Removed {
        tx_id: TxId,
        error: Error,
    },
}

pub struct PoolWorker<View> {
    thread_management_receiver: UnboundedReceiver<ThreadManagementRequest>,
    request_remove_receiver: UnboundedReceiver<PoolRemoveRequest>,
    request_read_receiver: UnboundedReceiver<PoolReadRequest>,
    extract_block_transactions_receiver: UnboundedReceiver<PoolExtractBlockTransactions>,
    request_insert_receiver: UnboundedReceiver<PoolInsertRequest>,
    pool: TxPool,
    view_provider: Arc<dyn AtomicView<LatestView = View>>,
    notification_sender: UnboundedSender<PoolNotification>,
}

impl<View> PoolWorker<View>
where
    View: TxPoolPersistentStorage,
{
    pub async fn run(&mut self) -> bool {
        let mut remove_buffer = vec![];
        let mut read_buffer = vec![];
        let mut insert_buffer = vec![];
        tokio::select! {
            biased;
            _ = self.thread_management_receiver.recv() => {
                return true
            }
            extract = self.extract_block_transactions_receiver.recv() => {
                match extract {
                    Some(PoolExtractBlockTransactions::ExtractBlockTransactions { constraints, transactions }) => {
                        self.get_block_transactions(constraints, transactions);
                    }
                    None => return true,
                }
            }
            _ = self.request_remove_receiver.recv_many(&mut remove_buffer, 1_000) => {
                for remove in remove_buffer {
                    match remove {
                        PoolRemoveRequest::Remove { tx_ids } => {
                            self.remove(tx_ids);
                        }
                        PoolRemoveRequest::RemoveCoinDependents { parent_txs } => {
                            self.remove_coin_dependents(parent_txs);
                        }
                        PoolRemoveRequest::RemoveAndCoinDependents { tx_ids } => {
                            self.remove_and_coin_dependents(tx_ids);
                        }
                    }
                }
            }
            _ = self.request_read_receiver.recv_many(&mut insert_buffer, 10_000) => {
                for read in insert_buffer {
                    match read {
                        PoolReadRequest::TxIds { max_txs, tx_ids } => {
                            self.get_tx_ids(max_txs, tx_ids);
                        }
                        PoolReadRequest::Txs { tx_ids, txs } => {
                            self.get_txs(tx_ids, txs);
                        }
                        PoolReadRequest::NonExistingTxs {
                            tx_ids,
                            non_existing_txs,
                        } => {
                            self.get_non_existing_txs(tx_ids, non_existing_txs);
                        }
                    }
                }
            }
            _ = self.request_insert_receiver.recv_many(&mut read_buffer, 1_000) => {
                for insert in read_buffer {
                    let PoolInsertRequest::Insert {
                        tx,
                        tx_for_p2p,
                        from_peer_info,
                        response_channel,
                    } = insert;

                    self.insert(tx, tx_for_p2p, from_peer_info, response_channel);
                }
            }
        }
        false
    }

    fn insert(
        &mut self,
        tx: ArcPoolTx,
        tx_for_p2p: Arc<Transaction>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    ) {
        let tx_id = tx.id();
        let expiration = tx.expiration();
        let result = self.view_provider.latest_view();
        let res = match result {
            Ok(view) => self.pool.insert(tx, &view),
            Err(err) => Err(Error::Database(format!("{:?}", err))),
        };

        match res {
            Ok(removed_txs) => {
                if let Err(e) =
                    self.notification_sender.send(PoolNotification::Inserted {
                        tx_id,
                        from_peer_info,
                        expiration,
                        time: SystemTime::now(),
                        tx: tx_for_p2p,
                    })
                {
                    tracing::error!("Failed to send inserted notification: {}", e);
                }

                for tx in removed_txs {
                    let removed_tx_id = tx.id();
                    if let Err(e) =
                        self.notification_sender.send(PoolNotification::Removed {
                            tx_id: removed_tx_id,
                            error: Error::Removed(RemovedReason::LessWorth(tx_id)),
                        })
                    {
                        tracing::error!("Failed to send removed notification: {}", e);
                    }
                }
                if let Some(channel) = response_channel {
                    let _ = channel.send(Ok(()));
                }
            }
            Err(error) => {
                if let Some(channel) = response_channel {
                    let _ = channel.send(Err(error.clone()));
                }
                if let Err(e) = self
                    .notification_sender
                    .send(PoolNotification::ErrorInsertion { tx_id, from_peer_info, error })
                {
                    tracing::error!("Failed to send error insertion notification: {}", e);
                }
            }
        }
    }

    fn get_block_transactions(
        &mut self,
        constraints: Constraints,
        blocks: oneshot::Sender<Vec<ArcPoolTx>>,
    ) {
        let txs = self.pool.extract_transactions_for_block(constraints);
        if blocks.send(txs).is_err() {
            tracing::error!("Failed to send block transactions");
        }
    }

    fn remove(&mut self, tx_ids: Vec<TxId>) {
        self.pool.remove_transaction(tx_ids);
    }

    fn remove_coin_dependents(&mut self, parent_txs: Vec<(TxId, String)>) {
        for (tx_id, reason) in parent_txs {
            let removed = self.pool.remove_coin_dependents(tx_id);
            for tx in removed {
                let tx_id = tx.id();
                if let Err(e) = self.notification_sender.send(PoolNotification::Removed {
                    tx_id,
                    error: Error::SkippedTransaction(
                        format!("Parent transaction with id: {tx_id}, was removed because of: {reason}")
                    )
                }) {
                    tracing::error!("Failed to send removed notification: {}", e);
                }
            }
        }
    }

    fn remove_and_coin_dependents(&mut self, tx_ids: (Vec<TxId>, Error)) {
        let error = tx_ids.1.clone();
        let removed = self.pool.remove_transaction_and_dependents(tx_ids.0);
        for tx in removed {
            let tx_id = tx.id();
            if let Err(e) = self.notification_sender.send(PoolNotification::Removed {
                tx_id,
                error: error.clone(),
            }) {
                tracing::error!("Failed to send removed notification: {}", e);
            }
        }
    }

    fn get_tx_ids(&mut self, max_txs: usize, tx_ids_sender: oneshot::Sender<Vec<TxId>>) {
        let tx_ids: Vec<TxId> = self.pool.iter_tx_ids().take(max_txs).copied().collect();
        if tx_ids_sender.send(tx_ids).is_err() {
            tracing::error!("Failed to send tx ids out of PoolWorker");
        }
    }

    fn get_txs(
        &mut self,
        tx_ids: Vec<TxId>,
        txs_sender: oneshot::Sender<Vec<Option<TxInfo>>>,
    ) {
        let txs: Vec<Option<TxInfo>> = tx_ids
            .into_iter()
            .map(|tx_id| {
                self.pool.find_one(&tx_id).map(|tx| TxInfo {
                    tx: tx.transaction.clone(),
                    creation_instant: tx.creation_instant,
                })
            })
            .collect();
        if txs_sender.send(txs).is_err() {
            tracing::error!("Failed to send txs from PoolWorker");
        }
    }

    fn get_non_existing_txs(
        &mut self,
        tx_ids: Vec<TxId>,
        non_existing_txs_sender: oneshot::Sender<Vec<TxId>>,
    ) {
        let non_existing_txs: Vec<TxId> = tx_ids
            .into_iter()
            .filter(|tx_id| !self.pool.contains(tx_id))
            .collect();
        if non_existing_txs_sender.send(non_existing_txs).is_err() {
            tracing::error!("Failed to send non existing txs");
        }
    }
}
