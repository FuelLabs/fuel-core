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
    ops::Deref,
    sync::Arc,
    time::SystemTime,
};
use tokio::sync::{
    mpsc,
    mpsc::{
        Receiver,
        Sender,
    },
    oneshot,
};

use crate::{
    config::ServiceChannelLimits,
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

const MAX_PENDING_READ_POOL_REQUESTS: usize = 10_000;
const MAX_PENDING_INSERT_POOL_REQUESTS: usize = 1_000;
const MAX_PENDING_REMOVE_POOL_REQUESTS: usize = 1_000;

const SIZE_EXTRACT_BLOCK_TRANSACTIONS_CHANNEL: usize = 100_000;
const SIZE_NOTIFICATION_CHANNEL: usize = 10_000_000;
const SIZE_THREAD_MANAGEMENT_CHANNEL: usize = 10;

pub(super) struct PoolWorkerInterface {
    thread_management_sender: Sender<ThreadManagementRequest>,
    pub(super) request_insert_sender: Sender<PoolInsertRequest>,
    pub(super) request_remove_sender: Sender<PoolRemoveRequest>,
    pub(super) request_read_sender: Sender<PoolReadRequest>,
    pub(super) extract_block_transactions_sender: Sender<PoolExtractBlockTransactions>,
    pub(super) notification_receiver: Receiver<PoolNotification>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl PoolWorkerInterface {
    pub fn new<View>(
        tx_pool: TxPool,
        view_provider: Arc<dyn AtomicView<LatestView = View>>,
        limits: &ServiceChannelLimits,
    ) -> Self
    where
        View: TxPoolPersistentStorage,
    {
        let (request_read_sender, request_read_receiver) =
            mpsc::channel(limits.max_pending_read_pool_requests);
        let (extract_block_transactions_sender, extract_block_transactions_receiver) =
            mpsc::channel(SIZE_EXTRACT_BLOCK_TRANSACTIONS_CHANNEL);
        let (request_remove_sender, request_remove_receiver) =
            mpsc::channel(limits.max_pending_write_pool_requests);
        let (request_insert_sender, request_insert_receiver) =
            mpsc::channel(limits.max_pending_write_pool_requests);
        let (notification_sender, notification_receiver) =
            mpsc::channel(SIZE_NOTIFICATION_CHANNEL);
        let (thread_management_sender, thread_management_receiver) =
            mpsc::channel(SIZE_THREAD_MANAGEMENT_CHANNEL);

        let handle = std::thread::spawn(move || {
            let Ok(tokio_runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                tracing::error!("Failed to build tokio runtime for pool worker");
                return;
            };
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

    pub fn remove_executed_transactions(&self, tx_ids: Vec<TxId>) -> anyhow::Result<()> {
        self.request_remove_sender
            .try_send(PoolRemoveRequest::ExecutedTransactions { tx_ids })
            .map_err(|e| anyhow::anyhow!("Failed to send remove request: {}", e))
    }

    pub fn remove_tx_and_coin_dependents(
        &self,
        tx_and_dependents_ids: (Vec<TxId>, Error),
    ) -> anyhow::Result<()> {
        self.request_remove_sender
            .try_send(PoolRemoveRequest::TxAndCoinDependents {
                tx_and_dependents_ids,
            })
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to send remove and coin dependents request: {}",
                    e
                )
            })
    }

    pub fn stop(&mut self) {
        if let Err(e) = self
            .thread_management_sender
            .try_send(ThreadManagementRequest::Stop)
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

enum ThreadManagementRequest {
    Stop,
}

#[allow(clippy::upper_case_acronyms)]
pub(super) enum InsertionSource {
    P2P {
        from_peer_info: GossipsubMessageInfo,
    },
    RPC {
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    },
}

pub(super) enum PoolInsertRequest {
    Insert {
        tx: ArcPoolTx,
        source: InsertionSource,
    },
}

pub(super) enum PoolExtractBlockTransactions {
    ExtractBlockTransactions {
        constraints: Constraints,
        transactions: oneshot::Sender<Vec<ArcPoolTx>>,
    },
}

pub(super) enum PoolRemoveRequest {
    ExecutedTransactions {
        tx_ids: Vec<TxId>,
    },
    CoinDependents {
        dependents_ids: Vec<(TxId, Error)>,
    },
    TxAndCoinDependents {
        tx_and_dependents_ids: (Vec<TxId>, Error),
    },
}
pub(super) enum PoolReadRequest {
    NonExistingTxs {
        tx_ids: Vec<TxId>,
        response_channel: oneshot::Sender<Vec<TxId>>,
    },
    Txs {
        tx_ids: Vec<TxId>,
        response_channel: oneshot::Sender<Vec<Option<TxInfo>>>,
    },
    TxIds {
        max_txs: usize,
        response_channel: oneshot::Sender<Vec<TxId>>,
    },
}

#[allow(clippy::upper_case_acronyms)]
pub(super) enum ExtendedInsertionSource {
    P2P {
        from_peer_info: GossipsubMessageInfo,
    },
    RPC {
        tx: Arc<Transaction>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    },
}

pub(super) enum PoolNotification {
    Inserted {
        tx_id: TxId,
        time: SystemTime,
        expiration: BlockHeight,
        source: ExtendedInsertionSource,
    },
    ErrorInsertion {
        tx_id: TxId,
        error: Error,
        source: InsertionSource,
    },
    Removed {
        tx_id: TxId,
        error: Error,
    },
}

pub(super) struct PoolWorker<View> {
    thread_management_receiver: Receiver<ThreadManagementRequest>,
    request_remove_receiver: Receiver<PoolRemoveRequest>,
    request_read_receiver: Receiver<PoolReadRequest>,
    extract_block_transactions_receiver: Receiver<PoolExtractBlockTransactions>,
    request_insert_receiver: Receiver<PoolInsertRequest>,
    pool: TxPool,
    view_provider: Arc<dyn AtomicView<LatestView = View>>,
    notification_sender: Sender<PoolNotification>,
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
            management_req = self.thread_management_receiver.recv() => {
                match management_req {
                    Some(req) => match req {
                        ThreadManagementRequest::Stop => return true,
                    },
                    None => return true,
                }
            }
            extract = self.extract_block_transactions_receiver.recv() => {
                match extract {
                    Some(PoolExtractBlockTransactions::ExtractBlockTransactions { constraints, transactions }) => {
                        self.extract_block_transactions(constraints, transactions);
                    }
                    None => return true,
                }
            }
            _ = self.request_remove_receiver.recv_many(&mut remove_buffer, MAX_PENDING_REMOVE_POOL_REQUESTS) => {
                if remove_buffer.is_empty() {
                    return true;
                }
                for remove in remove_buffer {
                    match remove {
                        PoolRemoveRequest::ExecutedTransactions { tx_ids } => {
                            self.remove(tx_ids);
                        }
                        PoolRemoveRequest::CoinDependents { dependents_ids } => {
                            self.remove_coin_dependents(dependents_ids);
                        }
                        PoolRemoveRequest::TxAndCoinDependents { tx_and_dependents_ids } => {
                            self.remove_and_coin_dependents(tx_and_dependents_ids);
                        }
                    }
                }
            }
            _ = self.request_read_receiver.recv_many(&mut read_buffer, MAX_PENDING_READ_POOL_REQUESTS) => {
                if read_buffer.is_empty() {
                    return true;
                }
                for read in read_buffer {
                    match read {
                        PoolReadRequest::TxIds { max_txs, response_channel } => {
                            self.get_tx_ids(max_txs, response_channel);
                        }
                        PoolReadRequest::Txs { tx_ids, response_channel } => {
                            self.get_txs(tx_ids, response_channel);
                        }
                        PoolReadRequest::NonExistingTxs {
                            tx_ids,
                            response_channel,
                        } => {
                            self.get_non_existing_txs(tx_ids, response_channel);
                        }
                    }
                }
            }
            _ = self.request_insert_receiver.recv_many(&mut insert_buffer, MAX_PENDING_INSERT_POOL_REQUESTS) => {
                if insert_buffer.is_empty() {
                    return true;
                }
                for insert in insert_buffer {
                    let PoolInsertRequest::Insert {
                        tx,
                        source,
                    } = insert;

                    self.insert(tx, source);
                }
            }
        }
        false
    }

    fn insert(&mut self, tx: ArcPoolTx, source: InsertionSource) {
        let tx_id = tx.id();
        let expiration = tx.expiration();
        let result = self.view_provider.latest_view();
        let res = match result {
            Ok(view) => self.pool.insert(tx, &view),
            Err(err) => Err(Error::Database(format!("{:?}", err))),
        };

        match res {
            Ok(removed_txs) => {
                let extended_source = match source {
                    InsertionSource::P2P { from_peer_info } => {
                        ExtendedInsertionSource::P2P { from_peer_info }
                    }
                    InsertionSource::RPC { response_channel } => {
                        let tx: Transaction = self
                            .pool
                            .get(&tx_id)
                            .expect("Transaction was inserted above; qed")
                            .transaction
                            .deref()
                            .into();

                        ExtendedInsertionSource::RPC {
                            tx: Arc::new(tx),
                            response_channel,
                        }
                    }
                };
                if let Err(e) =
                    self.notification_sender
                        .try_send(PoolNotification::Inserted {
                            tx_id,
                            expiration,
                            time: SystemTime::now(),
                            source: extended_source,
                        })
                {
                    tracing::error!("Failed to send inserted notification: {}", e);
                }

                for tx in removed_txs {
                    let removed_tx_id = tx.id();
                    if let Err(e) =
                        self.notification_sender
                            .try_send(PoolNotification::Removed {
                                tx_id: removed_tx_id,
                                error: Error::Removed(RemovedReason::LessWorth(tx_id)),
                            })
                    {
                        tracing::error!("Failed to send removed notification: {}", e);
                    }
                }
            }
            Err(error) => {
                if let Err(e) =
                    self.notification_sender
                        .try_send(PoolNotification::ErrorInsertion {
                            tx_id,
                            source,
                            error,
                        })
                {
                    tracing::error!("Failed to send error insertion notification: {}", e);
                }
            }
        }
    }

    fn extract_block_transactions(
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

    fn remove_coin_dependents(&mut self, parent_txs: Vec<(TxId, Error)>) {
        for (tx_id, reason) in parent_txs {
            let removed = self.pool.remove_coin_dependents(tx_id);
            for tx in removed {
                let tx_id = tx.id();
                if let Err(e) = self.notification_sender.try_send(PoolNotification::Removed {
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
        let (tx_ids, error) = tx_ids;
        let removed = self.pool.remove_transaction_and_dependents(tx_ids);
        for tx in removed {
            let tx_id = tx.id();
            if let Err(e) = self
                .notification_sender
                .try_send(PoolNotification::Removed {
                    tx_id,
                    error: error.clone(),
                })
            {
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
                self.pool.get(&tx_id).map(|tx| TxInfo {
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
        response_channel: oneshot::Sender<Vec<TxId>>,
    ) {
        let non_existing_txs: Vec<TxId> = tx_ids
            .into_iter()
            .filter(|tx_id| !self.pool.contains(tx_id))
            .collect();
        if response_channel.send(non_existing_txs).is_err() {
            tracing::error!("Failed to send non existing txs");
        }
    }
}
