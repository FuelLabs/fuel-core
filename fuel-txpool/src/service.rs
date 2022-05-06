use std::cmp::Reverse;
use std::sync::Arc;

use crate::Error;
use crate::{subscribers::MultiSubscriber, types::*, Config, TxPool as TxPoolImpl};
use async_trait::async_trait;
use fuel_core_interfaces::model::{ArcTx, TxInfo};
use fuel_core_interfaces::txpool::{Subscriber, TxPool, TxPoolDb};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct TxPoolService {
    txpool: RwLock<TxPoolImpl>,
    db: Box<dyn TxPoolDb>,
    subs: MultiSubscriber,
}

impl TxPoolService {
    pub fn new(db: Box<dyn TxPoolDb>, config: Config) -> Self {
        Self {
            txpool: RwLock::new(TxPoolImpl::new(config)),
            db,
            subs: MultiSubscriber::new(),
        }
    }
}

#[async_trait]
impl TxPool for TxPoolService {
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
                        self.subs.removed(removed.clone(), &Error::Removed).await;
                    }
                    self.subs.inserted(tx).await;
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
    /// This is going to be heavy operation, use it with only when needed.
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
        for removed in removed {
            self.subs.removed(removed, &Error::Removed).await
        }
    }

    async fn subscribe(&self, sub: Arc<dyn Subscriber>) {
        self.subs.sub(sub);
    }
}

#[cfg(any(test))]
pub mod tests {
    use super::*;
    use fuel_core_interfaces::db::helpers::*;

    #[tokio::test]
    async fn test_filter_by_negative() {
        let config = Config::default();
        let db = Box::new(DummyDB::filled());

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx3_hash = *TX_ID3;

        let tx1 = Arc::new(DummyDB::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDB::dummy_tx(tx2_hash));

        let service = TxPoolService::new(db, config);
        let out = service.insert(vec![tx1, tx2]).await;
        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        let out = service.filter_by_negative(&[tx1_hash, tx3_hash]).await;
        assert_eq!(out.len(), 1, "Shoud be len 1:{:?}", out);
        assert_eq!(out[0], tx3_hash, "Found tx id match{:?}", out);
    }

    #[tokio::test]
    async fn test_find() {
        let config = Config::default();
        let db = Box::new(DummyDB::filled());

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx3_hash = *TX_ID3;

        let tx1 = Arc::new(DummyDB::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDB::dummy_tx(tx2_hash));

        let service = TxPoolService::new(db, config);
        let out = service.insert(vec![tx1, tx2]).await;
        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        let out = service.find(&[tx1_hash, tx3_hash]).await;
        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_some(), "Tx1 should be some:{:?}", out);
        let id = out[0].as_ref().unwrap().id();
        assert_eq!(id, tx1_hash, "Found tx id match{:?}", out);
        assert!(out[1].is_none(), "Tx3 should not be found:{:?}", out);
    }

    #[tokio::test]
    async fn simple_insert_removal_subscription() {
        let config = Config::default();
        let db = Box::new(DummyDB::filled());

        struct Subs {
            pub new_tx: RwLock<Vec<ArcTx>>,
            pub rem_tx: RwLock<Vec<ArcTx>>,
        }

        #[async_trait]
        impl Subscriber for Subs {
            async fn inserted(&self, tx: ArcTx) {
                self.new_tx.write().await.push(tx);
            }

            async fn inserted_on_block_revert(&self, _tx: ArcTx) {}

            async fn removed(&self, tx: ArcTx, _error: &Error) {
                self.rem_tx.write().await.push(tx);
            }
        }

        let sub = Arc::new(Subs {
            new_tx: RwLock::new(Vec::new()),
            rem_tx: RwLock::new(Vec::new()),
        });

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;

        let tx1 = Arc::new(DummyDB::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDB::dummy_tx(tx2_hash));

        let service = TxPoolService::new(db, config);
        service.subscribe(sub.clone()).await;
        let out = service.insert(vec![tx1, tx2]).await;
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        {
            let added = sub.new_tx.read().await;
            assert_eq!(added.len(), 2, "Sub should contains two new tx");
            assert_eq!(added[0].id(), tx1_hash, "First added should be tx1");
            assert_eq!(added[1].id(), tx2_hash, "First added should be tx2");
        }
        service.remove(&[tx1_hash]).await;
        {
            // removing tx1 removed tx2, bcs it is dependent.
            let removed = sub.rem_tx.read().await;
            assert_eq!(removed.len(), 2, "Sub should contains two removed tx");
            assert_eq!(removed[0].id(), tx1_hash, "First removed should be tx1");
            assert_eq!(removed[1].id(), tx2_hash, "Second removed should be tx2");
        }
    }
}
