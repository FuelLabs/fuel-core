use crate::{
    containers::{dependency::Dependency, price_sort::PriceSort},
    types::*,
    Config, Error,
};
use fuel_core_interfaces::{
    model::{ArcTx, TxInfo},
    txpool::TxPoolDb,
};
use std::collections::HashMap;

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
    pub async fn insert(&mut self, tx: ArcTx, db: &dyn TxPoolDb) -> anyhow::Result<Vec<ArcTx>> {
        if tx.metadata().is_none() {
            return Err(Error::NoMetadata.into());
        }

        // verify gas price is at least the minimum
        self.verify_tx_min_gas_price(&tx)?;
        // verify byte price is at least the minimum
        self.verify_tx_min_byte_price(&tx)?;

        if self.by_hash.contains_key(&tx.id()) {
            return Err(Error::NotInsertedTxKnown.into());
        }

        let mut max_limit_hit = false;
        // check if we are hiting limit of pool
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

        // if some transaction were removed so we dont need to check limit
        if rem.is_empty() {
            if max_limit_hit {
                //remove last tx from sort
                let rem_tx = self.by_gas_price.last().unwrap(); // safe to unwrap limit is hit
                self.remove(&rem_tx);
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

    // When block is updated we need to receive all spend outputs and remove them from txpool
    pub fn block_update(&mut self /*spend_outputs: [Input], added_outputs: [AddedOutputs]*/) {}

    pub fn remove(&mut self, tx: &ArcTx) -> Vec<ArcTx> {
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

    fn verify_tx_min_byte_price(&mut self, tx: &Transaction) -> Result<(), Error> {
        if tx.byte_price() < self.config.min_byte_price {
            return Err(Error::NotInsertedBytePriceTooLow);
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::Error;
    use fuel_core_interfaces::{common::fuel_tx::UtxoId, db::helpers::*, model::CoinStatus};
    use std::cmp::Reverse;
    use std::sync::Arc;

    #[tokio::test]
    async fn simple_insertion() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);
        let out = txpool.insert(tx1, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1_faulty, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1.clone(), &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx1, &db).await;
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

        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx3, &db).await;
        assert!(out.is_ok(), "Tx3 should be okay:{:?}", out);
        let vec = out.ok().unwrap();
        assert!(!vec.is_empty(), "Tx1 should be removed:{:?}", vec);
        let tx_id = vec[0].id();
        assert_eq!(tx_id, tx1_hash, "Tx1 id should be removed");
    }

    #[tokio::test]
    async fn underpriced_tx1_not_included_coin_colision() {
        let config = Config::default();
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx3_hash = *TX_ID3;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx3 = Arc::new(DummyDb::dummy_tx(tx3_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert(tx3, &db).await;
        assert!(out.is_ok(), "Tx3 should be okay:{:?}", out);
        let out = txpool.insert(tx1, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be okay:{:?}", out);
        let out = txpool.insert(tx5, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be Ok:{:?}", out);
        let out = txpool.insert(tx5, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
        assert!(out.is_ok(), "Tx2 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx3, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2, &db).await;
        assert!(out.is_ok(), "Tx2 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx4, &db).await;
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

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
        let out = txpool.insert(tx2.clone(), &db).await;
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

        let out = txpool.insert(tx1, &db).await;
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

        let err = txpool.insert(tx1, &db).await.err().unwrap();
        assert!(matches!(
            err.root_cause().downcast_ref::<Error>().unwrap(),
            Error::NotInsertedGasPriceTooLow
        ));
    }

    #[tokio::test]
    async fn tx_above_min_byte_price_is_insertable() {
        let config = Config {
            min_byte_price: TX1_BYTE_PRICE,
            ..Config::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);

        let out = txpool.insert(tx1, &db).await;
        assert!(out.is_ok(), "Tx1 should be OK, get err:{:?}", out);
    }

    #[tokio::test]
    async fn tx_below_min_byte_price_is_not_insertable() {
        let config = Config {
            min_byte_price: TX1_BYTE_PRICE + 1,
            ..Config::default()
        };
        let db = DummyDb::filled();

        let tx1_hash = *TX_ID1;
        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));

        let mut txpool = TxPool::new(config);

        let err = txpool.insert(tx1, &db).await.err().unwrap();
        assert!(matches!(
            err.root_cause().downcast_ref::<Error>().unwrap(),
            Error::NotInsertedBytePriceTooLow
        ));
    }
}
