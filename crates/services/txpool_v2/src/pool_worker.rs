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
    service::TxPool,
};

pub struct PoolWorkerInterface {
    pub(crate) request_sender: Sender<PoolRequest>,
    pub(crate) notification_receiver: Receiver<PoolNotification>,
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
        let (notification_sender, notification_receiver) = std::sync::mpsc::channel();

        let handle = std::thread::spawn(move || {
            let mut worker = PoolWorker::new(
                request_receiver,
                notification_sender,
                tx_pool,
                view_provider,
            );
            worker.run();
        });
        Self {
            request_sender,
            notification_receiver,
            handle,
        }
    }

    pub fn insert(&self, tx: ArcPoolTx) {
        self.request_sender
            .send(PoolRequest::Insert { tx })
            .unwrap();
    }

    pub fn remove(&self, tx_ids: Vec<TxId>) {
        self.request_sender
            .send(PoolRequest::Remove { tx_ids })
            .unwrap();
    }

    pub fn remove_coin_dependents(&self, parent_txs: Vec<(TxId, String)>) {
        self.request_sender
            .send(PoolRequest::RemoveCoinDependents { parent_txs })
            .unwrap();
    }

    pub fn remove_and_coin_dependents(&self, tx_ids: (Vec<TxId>, Error)) {
        self.request_sender
            .send(PoolRequest::RemoveAndCoinDependents { tx_ids })
            .unwrap();
    }
}

pub enum PoolRequest {
    Insert { tx: ArcPoolTx },
    Remove { tx_ids: Vec<TxId> },
    RemoveCoinDependents { parent_txs: Vec<(TxId, String)> },
    RemoveAndCoinDependents { tx_ids: (Vec<TxId>, Error) },
}

pub enum PoolNotification {
    Inserted { tx_id: TxId, time: SystemTime },
    Removed { tx_id: TxId, error: Error },
}

pub struct PoolWorker<View> {
    request_receiver: Receiver<PoolRequest>,
    pool: TxPool,
    view_provider: Arc<dyn AtomicView<LatestView = View>>,
    notification_sender: Sender<PoolNotification>,
}

impl<View> PoolWorker<View> {
    pub fn new(
        request_receiver: Receiver<PoolRequest>,
        notification_sender: Sender<PoolNotification>,
        pool: TxPool,
        view_provider: Arc<dyn AtomicView<LatestView = View>>,
    ) -> Self {
        Self {
            request_receiver,
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
        while let Ok(request) = self.request_receiver.recv() {
            match request {
                PoolRequest::Insert { tx } => {
                    // TODO: Try to batch the insertions based on accumulation on interval
                    self.insert(tx);
                }
                PoolRequest::Remove { tx_ids } => {
                    self.remove(tx_ids);
                }
                PoolRequest::RemoveCoinDependents { parent_txs } => {
                    self.remove_coin_dependents(parent_txs);
                }
                PoolRequest::RemoveAndCoinDependents { tx_ids } => {
                    self.remove_and_coin_dependents(tx_ids);
                }
            }
        }
    }

    fn insert(&mut self, tx: ArcPoolTx) {
        let tx_id = tx.id();
        let result = self.view_provider.latest_view();
        let start_time = std::time::Instant::now();
        let res = match result {
            Ok(view) => self.pool.insert(tx, &view),
            Err(err) => Err(Error::Database(format!("{:?}", err))),
        };

        match res {
            Ok(removed_txs) => {
                self.notification_sender
                    .send(PoolNotification::Inserted {
                        tx_id,
                        time: SystemTime::now(),
                    })
                    .unwrap();
                for tx in removed_txs {
                    let tx_id = tx.id();
                    self.notification_sender
                        .send(PoolNotification::Removed {
                            tx_id,
                            error: Error::Removed(RemovedReason::LessWorth(tx_id)),
                        })
                        .unwrap();
                }
            }
            Err(err) => {
                self.notification_sender
                    .send(PoolNotification::Removed { tx_id, error: err })
                    .unwrap();
            }
        }
        tracing::info!(
            "Transaction (id: {}) took {} micros seconds to insert into the pool",
            &tx_id,
            start_time.elapsed().as_micros()
        );
    }

    fn remove(&mut self, tx_ids: Vec<TxId>) {
        self.pool.remove_transaction(tx_ids);
    }

    fn remove_coin_dependents(&mut self, parent_txs: Vec<(TxId, String)>) {
        for (tx_id, reason) in parent_txs {
            let removed = self.pool.remove_coin_dependents(tx_id);
            for tx in removed {
                let tx_id = tx.id();
                self.notification_sender.send(PoolNotification::Removed {
                    tx_id,
                    error: Error::SkippedTransaction(
                        format!("Parent transaction with {tx_id}, was removed because of the {reason}")
                    )
                }).unwrap();
            }
        }
    }

    fn remove_and_coin_dependents(&mut self, tx_ids: (Vec<TxId>, Error)) {
        let error = tx_ids.1.clone().to_string();
        let removed = self.pool.remove_transaction_and_dependents(tx_ids.0);
        for tx in removed {
            let tx_id = tx.id();
            self.notification_sender
                .send(PoolNotification::Removed {
                    tx_id,
                    error: Error::SkippedTransaction(format!(
                        "Transaction with {tx_id}, was removed because of the {error}"
                    )),
                })
                .unwrap();
        }
    }
}
