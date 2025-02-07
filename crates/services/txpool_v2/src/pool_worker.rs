#![allow(dead_code)]

use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::ArcPoolTx,
};
use std::{
    sync::{
        mpsc::{
            Receiver,
            Sender,
        },
        Arc,
    },
    time::SystemTime,
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
    pub(crate) insert_request_sender: Sender<PoolInsertRequest>,
    pub(crate) request_sender: Sender<PoolOtherRequest>,
    pub(crate) notification_receiver: tokio::sync::mpsc::Receiver<PoolNotification>,
    pub(crate) handle: std::thread::JoinHandle<()>,
}

impl PoolWorkerInterface {
    pub fn new<View>(
        tx_pool: TxPool,
        view_provider: Arc<dyn AtomicView<LatestView = View>>,
    ) -> Self
    where
        View: TxPoolPersistentStorage,
    {
        let (request_sender, request_receiver) = std::sync::mpsc::channel();
        let (insert_request_sender, insert_request_receiver) = std::sync::mpsc::channel();
        let (notification_sender, notification_receiver) =
            tokio::sync::mpsc::channel(100000);

        let handle = std::thread::spawn(move || {
            let mut worker = PoolWorker::new(
                request_receiver,
                insert_request_receiver,
                notification_sender,
                tx_pool,
                view_provider,
            );
            worker.run();
        });
        Self {
            request_sender,
            insert_request_sender,
            notification_receiver,
            handle,
        }
    }

    pub fn insert(
        &self,
        tx: ArcPoolTx,
        response_channel: Option<tokio::sync::oneshot::Sender<Result<(), Error>>>,
    ) {
        self.insert_request_sender
            .send(PoolInsertRequest::Insert {
                tx,
                response_channel,
            })
            .unwrap();
    }

    pub fn remove(&self, tx_ids: Vec<TxId>) {
        self.request_sender
            .send(PoolOtherRequest::Remove { tx_ids })
            .unwrap();
    }

    pub fn remove_coin_dependents(&self, parent_txs: Vec<(TxId, String)>) {
        self.request_sender
            .send(PoolOtherRequest::RemoveCoinDependents { parent_txs })
            .unwrap();
    }

    pub fn remove_and_coin_dependents(&self, tx_ids: (Vec<TxId>, Error)) {
        self.request_sender
            .send(PoolOtherRequest::RemoveAndCoinDependents { tx_ids })
            .unwrap();
    }

    pub fn get_block_transactions(
        &self,
        constraints: Constraints,
        blocks: Sender<Vec<ArcPoolTx>>,
    ) {
        self.request_sender
            .send(PoolOtherRequest::GetBlockTransactions {
                constraints,
                blocks,
            })
            .unwrap();
    }

    pub fn get_tx_ids(&self, max_txs: usize, tx_ids: Sender<Vec<TxId>>) {
        self.request_sender
            .send(PoolOtherRequest::GetTxIds { max_txs, tx_ids })
            .unwrap();
    }

    pub fn get_txs(&self, tx_ids: Vec<TxId>, txs: Sender<Vec<Option<TxInfo>>>) {
        self.request_sender
            .send(PoolOtherRequest::GetTxs { tx_ids, txs })
            .unwrap();
    }

    pub fn get_non_existing_txs(
        &self,
        tx_ids: Vec<TxId>,
        non_existing_txs: Sender<Vec<TxId>>,
    ) {
        self.request_sender
            .send(PoolOtherRequest::GetNonExistingTxs {
                tx_ids,
                non_existing_txs,
            })
            .unwrap();
    }
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
        response_channel: Option<tokio::sync::oneshot::Sender<Result<(), Error>>>,
    },
}

pub enum PoolOtherRequest {
    GetBlockTransactions {
        constraints: Constraints,
        blocks: Sender<Vec<ArcPoolTx>>,
    },
    Remove {
        tx_ids: Vec<TxId>,
    },
    RemoveCoinDependents {
        parent_txs: Vec<(TxId, String)>,
    },
    RemoveAndCoinDependents {
        tx_ids: (Vec<TxId>, Error),
    },
    GetNonExistingTxs {
        tx_ids: Vec<TxId>,
        non_existing_txs: Sender<Vec<TxId>>,
    },
    GetTxs {
        tx_ids: Vec<TxId>,
        txs: Sender<Vec<Option<TxInfo>>>,
    },
    GetTxIds {
        max_txs: usize,
        tx_ids: Sender<Vec<TxId>>,
    },
}

pub enum PoolNotification {
    Inserted { tx_id: TxId, time: SystemTime },
    Removed { tx_id: TxId, error: Error },
}

pub struct PoolWorker<View> {
    request_receiver: Receiver<PoolOtherRequest>,
    insert_request_receiver: Receiver<PoolInsertRequest>,
    pool: TxPool,
    view_provider: Arc<dyn AtomicView<LatestView = View>>,
    notification_sender: tokio::sync::mpsc::Sender<PoolNotification>,
}

impl<View> PoolWorker<View> {
    pub fn new(
        request_receiver: Receiver<PoolOtherRequest>,
        insert_request_receiver: Receiver<PoolInsertRequest>,
        notification_sender: tokio::sync::mpsc::Sender<PoolNotification>,
        pool: TxPool,
        view_provider: Arc<dyn AtomicView<LatestView = View>>,
    ) -> Self {
        Self {
            request_receiver,
            insert_request_receiver,
            pool,
            view_provider,
            notification_sender,
        }
    }
}

impl<View> PoolWorker<View>
where
    View: TxPoolPersistentStorage,
{
    pub fn run(&mut self) {
        // TODO: Try to find correct prio
        'outer: loop {
            loop {
                match self.insert_request_receiver.try_recv() {
                    Ok(PoolInsertRequest::Insert {
                        tx,
                        response_channel,
                    }) => {
                        self.insert(tx, response_channel);
                        continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break 'outer;
                    }
                }
            }

            match self.request_receiver.try_recv() {
                Ok(request) => match request {
                    PoolOtherRequest::GetBlockTransactions {
                        constraints,
                        blocks,
                    } => {
                        self.get_block_transactions(constraints, blocks);
                    }
                    PoolOtherRequest::Remove { tx_ids } => {
                        self.remove(tx_ids);
                    }
                    PoolOtherRequest::RemoveCoinDependents { parent_txs } => {
                        self.remove_coin_dependents(parent_txs);
                    }
                    PoolOtherRequest::RemoveAndCoinDependents { tx_ids } => {
                        self.remove_and_coin_dependents(tx_ids);
                    }
                    PoolOtherRequest::GetTxIds { max_txs, tx_ids } => {
                        self.get_tx_ids(max_txs, tx_ids);
                    }
                    PoolOtherRequest::GetTxs { tx_ids, txs } => {
                        self.get_txs(tx_ids, txs);
                    }
                    PoolOtherRequest::GetNonExistingTxs {
                        tx_ids,
                        non_existing_txs,
                    } => {
                        self.get_non_existing_txs(tx_ids, non_existing_txs);
                    }
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
    }

    fn insert(
        &mut self,
        tx: ArcPoolTx,
        response_channel: Option<tokio::sync::oneshot::Sender<Result<(), Error>>>,
    ) {
        let tx_id = tx.id();
        let result = self.view_provider.latest_view();
        // let start_time = std::time::Instant::now();
        let res = match result {
            Ok(view) => self.pool.insert(tx, &view),
            Err(err) => Err(Error::Database(format!("{:?}", err))),
        };

        match res {
            Ok(removed_txs) => {
                futures::executor::block_on(self.notification_sender.send(
                    PoolNotification::Inserted {
                        tx_id,
                        time: SystemTime::now(),
                    },
                ))
                .unwrap();
                for tx in removed_txs {
                    let removed_tx_id = tx.id();
                    futures::executor::block_on(self.notification_sender.send(
                        PoolNotification::Removed {
                            tx_id: removed_tx_id,
                            error: Error::Removed(RemovedReason::LessWorth(tx_id)),
                        },
                    ))
                    .unwrap();
                }
            }
            Err(err) => {
                futures::executor::block_on(
                    self.notification_sender
                        .send(PoolNotification::Removed { tx_id, error: err }),
                )
                .unwrap();
            }
        }
        if let Some(channel) = response_channel {
            let _ = channel.send(Ok(()));
        }
        // tracing::info!(
        //     "Transaction (id: {}) took {} micros seconds to insert into the pool",
        //     &tx_id,
        //     start_time.elapsed().as_micros()
        // );
    }

    fn get_block_transactions(
        &mut self,
        constraints: Constraints,
        blocks: Sender<Vec<ArcPoolTx>>,
    ) {
        let txs = self.pool.extract_transactions_for_block(constraints);
        // TODO: Unwrap
        blocks.send(txs).unwrap();
    }

    fn remove(&mut self, tx_ids: Vec<TxId>) {
        self.pool.remove_transaction(tx_ids);
    }

    fn remove_coin_dependents(&mut self, parent_txs: Vec<(TxId, String)>) {
        for (tx_id, reason) in parent_txs {
            let removed = self.pool.remove_coin_dependents(tx_id);
            for tx in removed {
                let tx_id = tx.id();
                futures::executor::block_on(self.notification_sender.send(PoolNotification::Removed {
                    tx_id,
                    error: Error::SkippedTransaction(
                        format!("Parent transaction with id: {tx_id}, was removed because of: {reason}")
                    )
                })).unwrap();
            }
        }
    }

    fn remove_and_coin_dependents(&mut self, tx_ids: (Vec<TxId>, Error)) {
        let error = tx_ids.1.clone().to_string();
        let removed = self.pool.remove_transaction_and_dependents(tx_ids.0);
        for tx in removed {
            let tx_id = tx.id();
            futures::executor::block_on(self.notification_sender.send(
                PoolNotification::Removed {
                    tx_id,
                    error: Error::SkippedTransaction(format!(
                        "Transaction with id: {tx_id}, was removed because of: {error}"
                    )),
                },
            ))
            .unwrap();
        }
    }

    fn get_tx_ids(&mut self, max_txs: usize, tx_ids_sender: Sender<Vec<TxId>>) {
        let tx_ids: Vec<TxId> = self
            .pool
            .iter_tx_ids()
            .take(max_txs)
            .map(|tx_id| *tx_id)
            .collect();
        tx_ids_sender.send(tx_ids).unwrap();
    }

    fn get_txs(&mut self, tx_ids: Vec<TxId>, txs_sender: Sender<Vec<Option<TxInfo>>>) {
        let txs: Vec<Option<TxInfo>> = tx_ids
            .into_iter()
            .map(|tx_id| {
                self.pool.find_one(&tx_id).map(|tx| TxInfo {
                    tx: tx.transaction.clone(),
                    creation_instant: tx.creation_instant,
                })
            })
            .collect();
        txs_sender.send(txs).unwrap();
    }

    fn get_non_existing_txs(
        &mut self,
        tx_ids: Vec<TxId>,
        non_existing_txs_sender: Sender<Vec<TxId>>,
    ) {
        let non_existing_txs: Vec<TxId> = tx_ids
            .into_iter()
            .filter(|tx_id| !self.pool.contains(tx_id))
            .collect();
        non_existing_txs_sender.send(non_existing_txs).unwrap();
    }
}
