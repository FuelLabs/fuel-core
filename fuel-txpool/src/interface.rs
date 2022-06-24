use crate::{types::*, Config, TxPool as TxPoolImpl};
use fuel_core_interfaces::block_importer::ImportBlockBroadcast;
use fuel_core_interfaces::model::{ArcTx, TxInfo};
use fuel_core_interfaces::txpool::{Error, TxPoolDb, TxPoolMpsc, TxStatus, TxStatusBroadcast};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Acts as a internal interface between transaction pool Service and implementation inside TxPool.
pub struct Interface {
    txpool: RwLock<TxPoolImpl>,
    broadcast: broadcast::Sender<TxStatusBroadcast>,
    db: Box<dyn TxPoolDb>,
}

impl Interface {
    pub async fn run(
        self: Arc<Interface>,
        mut new_block: broadcast::Receiver<ImportBlockBroadcast>,
        mut receiver: mpsc::Receiver<TxPoolMpsc>,
    ) -> mpsc::Receiver<TxPoolMpsc> {
        loop {
            tokio::select! {
                event = receiver.recv() => {
                    let event = event.unwrap();
                    if matches!(event,TxPoolMpsc::Stop) {
                        break;
                    }
                    let interface = self.clone();

                    // this is litlle bit risky but we can always add semaphore to limit number of requests.
                    tokio::spawn( async move {
                    match event {
                        TxPoolMpsc::Includable { response } => {
                            let _ = response.send(interface.includable().await);
                        }
                        TxPoolMpsc::Insert { txs, response } => {
                            let _ = response.send(interface.insert(txs).await);
                        }
                        TxPoolMpsc::Find { ids, response } => {
                            let _ = response.send(interface.find(&ids).await);
                        }
                        TxPoolMpsc::FindOne { id, response } => {
                            let _ = response.send(interface.find_one(&id).await);
                        }
                        TxPoolMpsc::FindDependent { ids, response } => {
                            let _ = response.send(interface.find_dependent(&ids).await);
                        }
                        TxPoolMpsc::FilterByNegative { ids, response } => {
                            let _ = response.send(interface.filter_by_negative(&ids).await);
                        }
                        TxPoolMpsc::Remove { ids } => {
                            let _ = interface.remove(&ids).await;
                        }
                        TxPoolMpsc::Stop => {}
                    }});
                }
                _block_updated = new_block.recv() => {
                    let interface = self.clone();
                    tokio::spawn( async move {
                        interface.block_update().await;
                    });
                }
            }
        }
        receiver
    }

    pub fn new(
        db: Box<dyn TxPoolDb>,
        broadcast: broadcast::Sender<TxStatusBroadcast>,
        config: Config,
    ) -> Self {
        Self {
            txpool: RwLock::new(TxPoolImpl::new(config)),
            broadcast,
            db,
        }
    }

    /// import tx
    async fn insert(&self, txs: Vec<ArcTx>) -> Vec<anyhow::Result<Vec<ArcTx>>> {
        // insert inside pool

        // Check that data is okay (witness match input/output, and if recovered signatures ara valid).
        // should be done before transaction comes to txpool, or before it enters RwLocked region.
        let mut res = Vec::new();
        for tx in txs.iter() {
            let mut pool = self.txpool.write().await;
            res.push(pool.insert(tx.clone(), self.db.as_ref()).await)
        }
        // announce to subscribers
        for (ret, tx) in res.iter().zip(txs.into_iter()) {
            match ret {
                Ok(removed) => {
                    for removed in removed {
                        // small todo there is possibility to have removal reason (ReplacedByHigherGas, DependencyRemoved)
                        // but for now it is okay to just use Error::Removed.
                        let _ = self.broadcast.send(TxStatusBroadcast {
                            tx: removed.clone(),
                            status: TxStatus::SqueezedOut {
                                reason: Error::Removed,
                            },
                        });
                    }
                    let _ = self.broadcast.send(TxStatusBroadcast {
                        tx,
                        status: TxStatus::Submitted,
                    });
                }
                Err(_) => {}
            }
        }
        res
    }

    /// find all tx by its hash
    async fn find(&self, hashes: &[TxId]) -> Vec<Option<TxInfo>> {
        let mut res = Vec::with_capacity(hashes.len());
        let pool = self.txpool.read().await;
        for hash in hashes {
            res.push(pool.txs().get(hash).cloned());
        }
        res
    }

    async fn find_one(&self, hash: &TxId) -> Option<TxInfo> {
        self.txpool.read().await.txs().get(hash).cloned()
    }

    /// find all dependent tx and return them with requsted dependencies in one list sorted by Price.
    async fn find_dependent(&self, hashes: &[TxId]) -> Vec<ArcTx> {
        let mut seen = HashMap::new();
        {
            let pool = self.txpool.read().await;
            for hash in hashes {
                if let Some(tx) = pool.txs().get(hash) {
                    pool.dependency()
                        .find_dependent(tx.tx().clone(), &mut seen, pool.txs());
                }
            }
        }
        let mut list: Vec<ArcTx> = seen.into_iter().map(|(_, tx)| tx).collect();
        // sort from high to low price
        list.sort_by_key(|tx| Reverse(tx.gas_price()));

        list
    }

    /// Iterete over `hashes` and return all hashes that we dont have.
    async fn filter_by_negative(&self, tx_ids: &[TxId]) -> Vec<TxId> {
        let mut res = Vec::new();
        let pool = self.txpool.read().await;
        for tx_id in tx_ids {
            if pool.txs().get(tx_id).is_none() {
                res.push(*tx_id)
            }
        }
        res
    }

    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    async fn includable(&self) -> Vec<ArcTx> {
        let pool = self.txpool.read().await;
        pool.sorted_includable()
    }

    /// When block is updated we need to receive all spend outputs and remove them from txpool
    async fn block_update(&self /*spend_outputs: [Input], added_outputs: [AddedOutputs]*/) {
        self.txpool.write().await.block_update()
    }

    /// remove transaction from pool needed on user demand. Low priority
    async fn remove(&self, tx_ids: &[TxId]) {
        let mut removed = Vec::new();
        for tx_id in tx_ids {
            let rem = { self.txpool.write().await.remove_by_tx_id(tx_id) };
            removed.extend(rem.into_iter());
        }
        for tx in removed {
            let _ = self.broadcast.send(TxStatusBroadcast {
                tx,
                status: TxStatus::SqueezedOut {
                    reason: Error::Removed,
                },
            });
        }
    }
}
