use crate::{
    containers::{dependency::Dependency, price_sort::PriceSort},
    types::*,
    Config, Error,
};
use fuel_core_interfaces::{
    model::{ArcTx, TxInfo},
    txpool::{TxPoolDb, TxStatus, TxStatusBroadcast},
};
use std::cmp::Reverse;
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone)]
pub struct TxPool {
    by_hash: HashMap<TxId, TxInfo>,
    by_gas_price: PriceSort,
    by_dependency: Dependency,
    config: Config,
}

impl TxPool {
    pub fn new(config: Config) -> Self {
        let max_depth = config.max_depth;
        Self {
            by_hash: HashMap::new(),
            by_gas_price: PriceSort::default(),
            by_dependency: Dependency::new(max_depth),
            config,
        }
    }
    pub fn txs(&self) -> &HashMap<TxId, TxInfo> {
        &self.by_hash
    }

    pub fn dependency(&self) -> &Dependency {
        &self.by_dependency
    }

    // this is atomic operation. Return removed(pushed out/replaced) transactions
    pub async fn insert_inner(
        &mut self,
        tx: ArcTx,
        db: &dyn TxPoolDb,
    ) -> anyhow::Result<Vec<ArcTx>> {
        if tx.metadata().is_none() {
            return Err(Error::NoMetadata.into());
        }

        // verify gas price is at least the minimum
        self.verify_tx_min_gas_price(&tx)?;

        if self.by_hash.contains_key(&tx.id()) {
            return Err(Error::NotInsertedTxKnown.into());
        }

        let mut max_limit_hit = false;
        // check if we are hitting limit of pool
        if self.by_hash.len() >= self.config.max_tx {
            max_limit_hit = true;
            // limit is hit, check if we can push out lowest priced tx
            let lowest_price = self.by_gas_price.lowest_price();
            if lowest_price >= tx.gas_price() {
                return Err(Error::NotInsertedLimitHit.into());
            }
        }
        // check and insert dependency
        let rem = self.by_dependency.insert(&self.by_hash, db, &tx).await?;
        self.by_hash.insert(tx.id(), TxInfo::new(tx.clone()));
        self.by_gas_price.insert(&tx);

        // if some transaction were removed so we don't need to check limit
        if rem.is_empty() {
            if max_limit_hit {
                //remove last tx from sort
                let rem_tx = self.by_gas_price.last().unwrap(); // safe to unwrap limit is hit
                self.remove_inner(&rem_tx);
                return Ok(vec![rem_tx]);
            }
            Ok(Vec::new())
        } else {
            // remove ret from by_hash and from by_price
            for rem in rem.iter() {
                self.by_hash
                    .remove(&rem.id())
                    .expect("Expect to hash of tx to be present");
                self.by_gas_price.remove(rem);
            }

            Ok(rem)
        }
    }

    /// Return all sorted transactions that are includable in next block.
    pub fn sorted_includable(&self) -> Vec<ArcTx> {
        self.by_gas_price
            .sort
            .iter()
            .rev()
            .map(|(_, tx)| tx.clone())
            .collect()
    }

    pub fn remove_inner(&mut self, tx: &ArcTx) -> Vec<ArcTx> {
        self.remove_by_tx_id(&tx.id())
    }

    /// remove transaction from pool needed on user demand. Low priority
    pub fn remove_by_tx_id(&mut self, tx_id: &TxId) -> Vec<ArcTx> {
        if let Some(tx) = self.by_hash.remove(tx_id) {
            let removed = self
                .by_dependency
                .recursively_remove_all_dependencies(&self.by_hash, tx.tx().clone());
            for remove in removed.iter() {
                self.by_gas_price.remove(remove);
                self.by_hash.remove(&remove.id());
            }
            return removed;
        }
        Vec::new()
    }

    fn verify_tx_min_gas_price(&mut self, tx: &Transaction) -> Result<(), Error> {
        if tx.gas_price() < self.config.min_gas_price {
            return Err(Error::NotInsertedGasPriceTooLow);
        }
        Ok(())
    }

    /// Import a set of transactions from network gossip or GraphQL endpoints.
    pub async fn insert(
        txpool: &RwLock<Self>,
        db: &dyn TxPoolDb,
        broadcast: broadcast::Sender<TxStatusBroadcast>,
        txs: Vec<ArcTx>,
    ) -> Vec<anyhow::Result<Vec<ArcTx>>> {
        // Check if that data is okay (witness match input/output, and if recovered signatures ara valid).
        // should be done before transaction comes to txpool, or before it enters RwLocked region.
        let mut res = Vec::new();
        for tx in txs.iter() {
            let mut pool = txpool.write().await;
            res.push(pool.insert_inner(tx.clone(), db).await)
        }
        // announce to subscribers
        for (ret, tx) in res.iter().zip(txs.into_iter()) {
            match ret {
                Ok(removed) => {
                    for removed in removed {
                        // small todo there is possibility to have removal reason (ReplacedByHigherGas, DependencyRemoved)
                        // but for now it is okay to just use Error::Removed.
                        let _ = broadcast.send(TxStatusBroadcast {
                            tx: removed.clone(),
                            status: TxStatus::SqueezedOut {
                                reason: Error::Removed,
                            },
                        });
                    }
                    let _ = broadcast.send(TxStatusBroadcast {
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
    pub async fn find(txpool: &RwLock<Self>, hashes: &[TxId]) -> Vec<Option<TxInfo>> {
        let mut res = Vec::with_capacity(hashes.len());
        let pool = txpool.read().await;
        for hash in hashes {
            res.push(pool.txs().get(hash).cloned());
        }
        res
    }

    pub async fn find_one(txpool: &RwLock<Self>, hash: &TxId) -> Option<TxInfo> {
        txpool.read().await.txs().get(hash).cloned()
    }

    /// find all dependent tx and return them with requested dependencies in one list sorted by Price.
    pub async fn find_dependent(txpool: &RwLock<Self>, hashes: &[TxId]) -> Vec<ArcTx> {
        let mut seen = HashMap::new();
        {
            let pool = txpool.read().await;
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

    /// Iterate over `hashes` and return all hashes that we don't have.
    pub async fn filter_by_negative(txpool: &RwLock<Self>, tx_ids: &[TxId]) -> Vec<TxId> {
        let mut res = Vec::new();
        let pool = txpool.read().await;
        for tx_id in tx_ids {
            if pool.txs().get(tx_id).is_none() {
                res.push(*tx_id)
            }
        }
        res
    }

    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    pub async fn includable(txpool: &RwLock<Self>) -> Vec<ArcTx> {
        let pool = txpool.read().await;
        pool.sorted_includable()
    }

    /// When block is updated we need to receive all spend outputs and remove them from txpool.
    pub async fn block_update(
        txpool: &RwLock<Self>, /*spend_outputs: [Input], added_outputs: [AddedOutputs]*/
    ) {
        txpool.write().await;
        // TODO https://github.com/FuelLabs/fuel-core/issues/465
    }

    /// remove transaction from pool needed on user demand. Low priority
    pub async fn remove(
        txpool: &RwLock<Self>,
        broadcast: broadcast::Sender<TxStatusBroadcast>,
        tx_ids: &[TxId],
    ) -> Vec<ArcTx> {
        let mut removed = Vec::new();
        for tx_id in tx_ids {
            let rem = { txpool.write().await.remove_by_tx_id(tx_id) };
            removed.extend(rem.into_iter());
        }
        for tx in &removed {
            let _ = broadcast.send(TxStatusBroadcast {
                tx: tx.clone(),
                status: TxStatus::SqueezedOut {
                    reason: Error::Removed,
                },
            });
        }
        removed
    }
}

#[cfg(test)]
pub mod tests {
    mod helpers {
        use fuel_core_interfaces::common::fuel_tx::Input;
        use fuel_core_interfaces::{
            common::{
                fuel_storage::Storage,
                fuel_tx::{Contract, ContractId, MessageId, UtxoId},
            },
            db::{self, KvStoreError},
            model::{Coin, Message},
            txpool::TxPoolDb,
        };
        use std::{
            borrow::Cow,
            collections::HashMap,
            sync::{Arc, Mutex},
        };

        #[derive(Default)]
        pub(crate) struct Data {
            pub coins: HashMap<UtxoId, Coin>,
            pub contracts: HashMap<ContractId, Contract>,
            pub messages: HashMap<MessageId, Message>,
        }

        #[derive(Default)]
        pub(crate) struct MockDb {
            pub data: Arc<Mutex<Data>>,
        }

        impl Storage<UtxoId, Coin> for MockDb {
            type Error = KvStoreError;

            fn insert(&mut self, key: &UtxoId, value: &Coin) -> Result<Option<Coin>, Self::Error> {
                Ok(self.data.lock().unwrap().coins.insert(*key, value.clone()))
            }

            fn remove(&mut self, key: &UtxoId) -> Result<Option<Coin>, Self::Error> {
                Ok(self.data.lock().unwrap().coins.remove(key))
            }

            fn get<'a>(&'a self, key: &UtxoId) -> Result<Option<Cow<'a, Coin>>, Self::Error> {
                Ok(self
                    .data
                    .lock()
                    .unwrap()
                    .coins
                    .get(key)
                    .map(|i| Cow::Owned(i.clone())))
            }

            fn contains_key(&self, key: &UtxoId) -> Result<bool, Self::Error> {
                Ok(self.data.lock().unwrap().coins.contains_key(key))
            }
        }

        impl Storage<ContractId, Contract> for MockDb {
            type Error = db::Error;

            fn insert(
                &mut self,
                key: &ContractId,
                value: &Contract,
            ) -> Result<Option<Contract>, Self::Error> {
                Ok(self
                    .data
                    .lock()
                    .unwrap()
                    .contracts
                    .insert(*key, value.clone()))
            }

            fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Self::Error> {
                Ok(self.data.lock().unwrap().contracts.remove(key))
            }

            fn get<'a>(
                &'a self,
                key: &ContractId,
            ) -> Result<Option<Cow<'a, Contract>>, Self::Error> {
                Ok(self
                    .data
                    .lock()
                    .unwrap()
                    .contracts
                    .get(key)
                    .map(|i| Cow::Owned(i.clone())))
            }

            fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
                Ok(self.data.lock().unwrap().contracts.contains_key(key))
            }
        }

        impl Storage<MessageId, Message> for MockDb {
            type Error = db::KvStoreError;

            fn insert(
                &mut self,
                key: &MessageId,
                value: &Message,
            ) -> Result<Option<Message>, Self::Error> {
                Ok(self
                    .data
                    .lock()
                    .unwrap()
                    .messages
                    .insert(*key, value.clone()))
            }

            fn remove(&mut self, key: &MessageId) -> Result<Option<Message>, Self::Error> {
                Ok(self.data.lock().unwrap().messages.remove(key))
            }

            fn get<'a>(&'a self, key: &MessageId) -> Result<Option<Cow<'a, Message>>, Self::Error> {
                Ok(self
                    .data
                    .lock()
                    .unwrap()
                    .messages
                    .get(key)
                    .map(|i| Cow::Owned(i.clone())))
            }

            fn contains_key(&self, key: &MessageId) -> Result<bool, Self::Error> {
                Ok(self.data.lock().unwrap().messages.contains_key(key))
            }
        }

        impl TxPoolDb for MockDb {}

        pub(crate) fn create_message_predicate_from_message(message: &Message) -> Input {
            Input::message_predicate(
                message.id(),
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                message.owner,
                message.data.clone(),
                Default::default(),
                Default::default(),
            )
        }
    }

    use super::*;
    use crate::Error;
    use fuel_core_interfaces::{
        common::{
            fuel_storage::Storage,
            fuel_tx::{TransactionBuilder, UtxoId},
        },
        db::helpers::*,
        model::{CoinStatus, Message},
    };
    use std::{cmp::Reverse, sync::Arc};

    #[tokio::test]
    async fn simple_insertion() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);
        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Transaction should be OK, get err:{:?}", out);
    }

    #[tokio::test]
    async fn simple_dependency_tx1_tx2() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_ok(), "Tx2 dependent should be OK, get err:{:?}", out);
    }

    #[tokio::test]
    async fn faulty_t2_collided_on_contract_id_from_tx1() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID_FAULTY2;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_err(), "Tx2 should collide on ContractId");
        assert_eq!(out.err().unwrap().to_string(),
        "Transaction is not inserted. More priced tx has created contract with ContractId 0x0000000000000000000000000000000000000000000000000000000000000100"
    );
    }

    #[tokio::test]
    async fn fails_to_insert_tx2_with_missing_utxo_dependency_on_faulty_tx1() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_faulty_hash = *TX_ID_FAULTY1;
        let tx2_hash = *TX_ID2;

        let tx1_faulty = Arc::new(DummyDb::dummy_tx(tx1_faulty_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1_faulty, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_err(), "Tx2 should be error");
        assert_eq!(out.err().unwrap().to_string(),"Transaction is not inserted. UTXO is not existing: 0x000000000000000000000000000000000000000000000000000000000000001000");
    }

    #[tokio::test]
    async fn not_inserted_known_tx() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1 = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1.clone(), &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_err(), "Second insertion of Tx1 should be error");
        assert_eq!(
            out.err().unwrap().to_string(),
            "Transaction is not inserted. Hash is already known"
        );
    }

    #[tokio::test]
    async fn try_to_insert_tx2_missing_utxo() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx2_hash = *TX_ID2;
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_err(), "Tx2 should be error");
        assert_eq!(out.err().unwrap().to_string(),"Transaction is not inserted. UTXO is not existing: 0x000000000000000000000000000000000000000000000000000000000000001000",);
    }

    #[tokio::test]
    async fn tx1_try_to_use_spend_coin() {
        let config = Config::default();
        let db = DummyDb::filled();

        // mark utxo as spend
        db.data
            .lock()
            .coins
            .get_mut(&UtxoId::new(*TX_ID_DB1, 0))
            .unwrap()
            .status = CoinStatus::Spent;

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_err(), "Tx1 should be error");
        assert_eq!(out.err().unwrap().to_string(),"Transaction is not inserted. UTXO is spent: 0x000000000000000000000000000000000000000000000000000000000000000000",);
    }

    #[tokio::test]
    async fn more_priced_tx3_removes_tx1() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx3_hash = *TX_ID3;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx3 = Arc::new(DummyDb::dummy_tx(tx3_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx3, &db).await;
        assert!(out.is_ok(), "Tx3 should be okay:{:?}", out);
        let vec = out.ok().unwrap();
        assert!(!vec.is_empty(), "Tx1 should be removed:{:?}", vec);
        let tx_id = vec[0].id();
        assert_eq!(tx_id, tx1_hash, "Tx1 id should be removed");
    }

    #[tokio::test]
    async fn underpriced_tx1_not_included_coin_collision() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx3_hash = *TX_ID3;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx3 = Arc::new(DummyDb::dummy_tx(tx3_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx3, &db).await;
        assert!(out.is_ok(), "Tx3 should be okay:{:?}", out);
        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_err(), "Tx1 should be ERR");
        let err = out.err().unwrap();
        assert_eq!(err.to_string(),"Transaction is not inserted. More priced tx 0x0000000000000000000000000000000000000000000000000000000000000012 already spend this UTXO output: 0x000000000000000000000000000000000000000000000000000000000000000000", "Tx1 should not be included:{:?}",err);
    }

    #[tokio::test]
    async fn overpriced_tx5_contract_input_not_inserted() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx5_hash = *TX_ID5;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let mut tx5 = DummyDb::dummy_tx(tx5_hash);
        tx5.set_gas_price(tx1.gas_price() + 1);
        tx5.precompute_metadata();
        let tx5 = Arc::new(tx5);

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be okay:{:?}", out);
        let out = txpool.insert_inner(tx5, &db).await;
        assert!(out.is_err(), "Tx5 should be ERR");
        let err = out.err().unwrap();
        assert_eq!(err.to_string(),"Transaction is not inserted. UTXO requires Contract input 0x0000000000000000000000000000000000000000000000000000000000000100 that is priced lower", "Tx5 should not be included:{:?}",err);
    }

    #[tokio::test]
    async fn dependent_tx5_contract_input_inserted() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx5_hash = *TX_ID5;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx5 = Arc::new(DummyDb::dummy_tx(tx5_hash));
        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be Ok:{:?}", out);
        let out = txpool.insert_inner(tx5, &db).await;
        assert!(out.is_ok(), "Tx5 should be Ok:{:?}", out);
    }

    #[tokio::test]
    async fn more_priced_tx3_removes_tx1_and_dependent_tx2() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx3_hash = *TX_ID3;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));
        let tx3 = Arc::new(DummyDb::dummy_tx(tx3_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_ok(), "Tx2 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx3, &db).await;
        assert!(out.is_ok(), "Tx3 should be okay:{:?}", out);
        let vec = out.ok().unwrap();
        assert_eq!(vec.len(), 2, "Tx1 and Tx2 should be removed:{:?}", vec);
        assert_eq!(vec[0].id(), tx1_hash, "Tx1 id should be removed");
        assert_eq!(vec[1].id(), tx2_hash, "Tx2 id should be removed");
    }

    #[tokio::test]
    async fn tx_limit_hit() {
        let config = Config {
            max_tx: 1,
            max_depth: 10,
            ..Default::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));
        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        let t: Error = out.unwrap_err().downcast().unwrap();
        assert_eq!(t, Error::NotInsertedLimitHit, "Tx2 should hit number limit");
    }

    #[tokio::test]
    async fn tx2_depth_hit() {
        let config = Config {
            max_tx: 10,
            max_depth: 1,
            ..Default::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));
        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        let t: Error = out.unwrap_err().downcast().unwrap();
        assert_eq!(t, Error::NotInsertedMaxDepth, "Tx2 should hit max depth");
    }

    #[tokio::test]
    async fn sorted_out_tx1_2_4() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx4_hash = *TX_ID4;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));
        let tx4 = Arc::new(DummyDb::dummy_tx(tx4_hash));
        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2, &db).await;
        assert!(out.is_ok(), "Tx2 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx4, &db).await;
        assert!(out.is_ok(), "Tx4 should be OK, get err:{:?}", out);

        let txs = txpool.sorted_includable();

        assert_eq!(txs.len(), 3, "Should have 3 txs");
        assert_eq!(txs[0].id(), tx4_hash, "First should be tx4");
        assert_eq!(txs[1].id(), tx1_hash, "Second should be tx1");
        assert_eq!(txs[2].id(), tx2_hash, "Third should be tx2");
    }

    #[tokio::test]
    async fn find_dependent_tx1_tx2() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));
        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert_inner(tx2.clone(), &db).await;
        assert!(out.is_ok(), "Tx2 should be OK, get err:{:?}", out);
        let mut seen = HashMap::new();
        txpool
            .dependency()
            .find_dependent(tx2, &mut seen, txpool.txs());
        let mut list: Vec<ArcTx> = seen.into_iter().map(|(_, tx)| tx).collect();
        // sort from high to low price
        list.sort_by_key(|tx| Reverse(tx.gas_price()));
        assert_eq!(list.len(), 2, "We should have two items");
        assert_eq!(list[0].id(), tx1_hash, "Tx1 should be first.");
        assert_eq!(list[1].id(), tx2_hash, "Tx2 should be second.");
    }

    #[tokio::test]
    async fn tx_at_least_min_gas_price_is_insertable() {
        let config = Config {
            min_gas_price: TX1_GAS_PRICE,
            ..Config::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert_inner(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
    }

    #[tokio::test]
    async fn tx_below_min_gas_price_is_not_insertable() {
        let config = Config {
            min_gas_price: TX1_GAS_PRICE + 1,
            ..Config::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);

        let err = txpool.insert_inner(tx1, &db).await.err().unwrap();
        assert!(matches!(
            err.root_cause().downcast_ref::<Error>().unwrap(),
            Error::NotInsertedGasPriceTooLow
        ));
    }

    #[tokio::test]
    async fn tx_inserted_into_pool_when_input_message_id_exists_in_db() {
        let message = Message {
            ..Default::default()
        };

        let tx = TransactionBuilder::script(vec![], vec![])
            .add_input(helpers::create_message_predicate_from_message(&message))
            .finalize();

        let mut db = helpers::MockDb::default();
        db.insert(&message.id(), &message).unwrap();
        let mut txpool = TxPool::new(Default::default());

        txpool
            .insert_inner(Arc::new(tx.clone()), &db)
            .await
            .expect("should succeed");

        let returned_tx = TxPool::find_one(&RwLock::new(txpool), &tx.id()).await;
        let tx_info = returned_tx.unwrap();
        assert_eq!(tx_info.tx().id(), tx.id());
    }

    #[tokio::test]
    async fn tx_rejected_when_input_message_id_is_spent() {
        let message = Message {
            fuel_block_spend: Some(1u64.into()),
            ..Default::default()
        };

        let tx = TransactionBuilder::script(vec![], vec![])
            .add_input(helpers::create_message_predicate_from_message(&message))
            .finalize();

        let mut db = helpers::MockDb::default();
        db.insert(&message.id(), &message).unwrap();
        let mut txpool = TxPool::new(Default::default());

        let err = txpool
            .insert_inner(Arc::new(tx.clone()), &db)
            .await
            .expect_err("should fail");

        // check error
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedInputMessageIdSpent(msg_id)) if msg_id == &message.id()
        ));
    }

    #[tokio::test]
    async fn tx_rejected_from_pool_when_input_message_id_does_not_exist_in_db() {
        let message = Message::default();
        let tx = TransactionBuilder::script(vec![], vec![])
            .add_input(helpers::create_message_predicate_from_message(&message))
            .finalize();

        let db = helpers::MockDb::default();
        // Do not insert any messages into the DB to ensure there is no matching message for the
        // tx.

        let mut txpool = TxPool::new(Default::default());

        let err = txpool
            .insert_inner(Arc::new(tx.clone()), &db)
            .await
            .expect_err("should fail");

        // check error
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedInputMessageUnknown(msg_id)) if msg_id == &message.id()
        ));
    }

    #[tokio::test]
    async fn tx_rejected_from_pool_when_gas_price_is_lower_than_another_tx_with_same_message_id() {
        let message_amount = 10_000;
        let message = Message {
            amount: message_amount,
            ..Default::default()
        };

        let conflicting_message_input = helpers::create_message_predicate_from_message(&message);
        let gas_price_high = 2u64;
        let gas_price_low = 1u64;

        let tx_high = TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price_high)
            .add_input(conflicting_message_input.clone())
            .finalize();

        let tx_low = TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price_low)
            .add_input(conflicting_message_input)
            .finalize();

        let mut db = helpers::MockDb::default();
        db.insert(&message.id(), &message).unwrap();

        let mut txpool = TxPool::new(Default::default());

        // Insert a tx for the message id with a high gas amount
        txpool
            .insert_inner(Arc::new(tx_high.clone()), &db)
            .await
            .expect("expected successful insertion");

        // Insert a tx for the message id with a low gas amount
        // Because the new transaction's id matches an existing transaction, we compare the gas
        // prices of both the new and existing transactions. Since the existing transaction's gas
        // price is higher, we must now reject the new transaction.
        let err = txpool
            .insert_inner(Arc::new(tx_low.clone()), &db)
            .await
            .expect_err("expected failure");

        // check error
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedCollisionMessageId(tx_id, msg_id)) if tx_id == &tx_high.id() && msg_id == &message.id()
        ));
    }

    #[tokio::test]
    async fn higher_priced_tx_squeezes_out_lower_priced_tx_with_same_message_id() {
        let message_amount = 10_000;
        let message = Message {
            amount: message_amount,
            ..Default::default()
        };

        let conflicting_message_input = helpers::create_message_predicate_from_message(&message);
        let gas_price_high = 2u64;
        let gas_price_low = 1u64;

        // Insert a tx for the message id with a low gas amount
        let tx_low = TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price_low)
            .add_input(conflicting_message_input.clone())
            .finalize();

        let mut db = helpers::MockDb::default();
        db.insert(&message.id(), &message).unwrap();

        let mut txpool = TxPool::new(Default::default());

        txpool
            .insert_inner(Arc::new(tx_low.clone()), &db)
            .await
            .expect("should succeed");

        // Insert a tx for the message id with a high gas amount
        // Because the new transaction's id matches an existing transaction, we compare the gas
        // prices of both the new and existing transactions. Since the existing transaction's gas
        // price is lower, we accept the new transaction and squeeze out the old transaction.
        let tx_high = TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price_high)
            .add_input(conflicting_message_input)
            .finalize();

        let squeezed_out_txs = txpool
            .insert_inner(Arc::new(tx_high.clone()), &db)
            .await
            .expect("should succeed");

        assert_eq!(squeezed_out_txs.len(), 1);
        assert_eq!(squeezed_out_txs[0].id(), tx_low.id());
    }

    #[tokio::test]
    async fn message_of_squeezed_out_tx_can_be_resubmitted_at_lower_gas_price() {
        // tx1 (message 1, message 2) gas_price 2
        // tx2 (message 1) gas_price 3
        //   squeezes tx1 with higher gas price
        // tx3 (message 2) gas_price 1
        //   works since tx1 is no longer part of txpool state even though gas price is less

        let message_1 = Message {
            amount: 10_000,
            ..Default::default()
        };
        let message_2 = Message {
            amount: 20_000,
            ..Default::default()
        };

        let message_input_1 = helpers::create_message_predicate_from_message(&message_1);
        let message_input_2 = helpers::create_message_predicate_from_message(&message_2);

        // Insert a tx for the message id with a low gas amount
        let tx_1 = TransactionBuilder::script(vec![], vec![])
            .gas_price(2)
            .add_input(message_input_1.clone())
            .add_input(message_input_2.clone())
            .finalize();

        let tx_2 = TransactionBuilder::script(vec![], vec![])
            .gas_price(3)
            .add_input(message_input_1.clone())
            .finalize();

        let tx_3 = TransactionBuilder::script(vec![], vec![])
            .gas_price(1)
            .add_input(message_input_2.clone())
            .finalize();

        let mut db = helpers::MockDb::default();
        db.insert(&message_1.id(), &message_1).unwrap();
        db.insert(&message_2.id(), &message_2).unwrap();
        let mut txpool = TxPool::new(Default::default());

        txpool
            .insert_inner(Arc::new(tx_1.clone()), &db)
            .await
            .expect("should succeed");

        txpool
            .insert_inner(Arc::new(tx_2.clone()), &db)
            .await
            .expect("should succeed");

        txpool
            .insert_inner(Arc::new(tx_3.clone()), &db)
            .await
            .expect("should succeed");
    }
}
