use crate::{
    containers::{
        dependency::Dependency,
        price_sort::PriceSort,
    },
    types::*,
    Config,
    Error,
};
use fuel_core_interfaces::{
    model::{
        ArcTx,
        FuelBlock,
        TxInfo,
    },
    txpool::{
        TxPoolDb,
        TxStatus,
        TxStatusBroadcast,
    },
};
use std::{
    cmp::Reverse,
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::{
    broadcast,
    RwLock,
};

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
            return Err(Error::NoMetadata.into())
        }

        // verify gas price is at least the minimum
        self.verify_tx_min_gas_price(&tx)?;

        if self.by_hash.contains_key(&tx.id()) {
            return Err(Error::NotInsertedTxKnown.into())
        }

        let mut max_limit_hit = false;
        // check if we are hitting limit of pool
        if self.by_hash.len() >= self.config.max_tx {
            max_limit_hit = true;
            // limit is hit, check if we can push out lowest priced tx
            let lowest_price = self.by_gas_price.lowest_price();
            if lowest_price >= tx.gas_price() {
                return Err(Error::NotInsertedLimitHit.into())
            }
        }
        // check and insert dependency
        let rem = self.by_dependency.insert(&self.by_hash, db, &tx).await?;
        self.by_hash.insert(tx.id(), TxInfo::new(tx.clone()));
        self.by_gas_price.insert(&tx);

        // if some transaction were removed so we don't need to check limit
        if rem.is_empty() {
            if max_limit_hit {
                // remove last tx from sort
                let rem_tx = self.by_gas_price.last().unwrap(); // safe to unwrap limit is hit
                self.remove_inner(&rem_tx);
                return Ok(vec![rem_tx])
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
            return removed
        }
        Vec::new()
    }

    fn verify_tx_min_gas_price(&mut self, tx: &Transaction) -> Result<(), Error> {
        if tx.gas_price() < self.config.min_gas_price {
            return Err(Error::NotInsertedGasPriceTooLow)
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
                    pool.dependency().find_dependent(
                        tx.tx().clone(),
                        &mut seen,
                        pool.txs(),
                    );
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
        txpool: &RwLock<Self>,
        block: Arc<FuelBlock>,
        // spend_outputs: [Input], added_outputs: [AddedOutputs]
    ) {
        let mut guard = txpool.write().await;
        // TODO https://github.com/FuelLabs/fuel-core/issues/465

        for tx in &block.transactions {
            let _removed = guard.remove_by_tx_id(&tx.id());
        }
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
    use super::*;
    use crate::{
        txpool::tests::helpers::{
            create_coin_input,
            create_coin_output,
            create_contract_input,
            create_contract_output,
        },
        Error,
        MockDb,
    };
    use fuel_core_interfaces::{
        common::{
            fuel_storage::StorageAsMut,
            fuel_tx::{
                TransactionBuilder,
                UtxoId,
            },
        },
        db::{
            Coins,
            Messages,
        },
        model::{
            BlockHeight,
            Coin,
            CoinStatus,
            Message,
        },
    };
    use std::{
        cmp::Reverse,
        str::FromStr,
        sync::Arc,
    };

    mod helpers {
        use crate::types::TxId;
        use fuel_core_interfaces::{
            common::fuel_tx::{
                Contract,
                ContractId,
                Input,
                Output,
                UtxoId,
            },
            model::Message,
        };

        pub(crate) fn create_message_predicate_from_message(message: &Message) -> Input {
            Input::message_predicate(
                message.id(),
                message.sender,
                message.recipient,
                message.amount,
                message.nonce,
                message.data.clone(),
                Default::default(),
                Default::default(),
            )
        }

        pub(crate) fn create_coin_input(tx_id: TxId, output_index: u8) -> Input {
            Input::CoinSigned {
                utxo_id: UtxoId::new(tx_id, output_index),
                owner: Default::default(),
                amount: Default::default(),
                asset_id: Default::default(),
                tx_pointer: Default::default(),
                witness_index: Default::default(),
                maturity: Default::default(),
            }
        }

        pub(crate) fn create_coin_output() -> Output {
            Output::Coin {
                amount: Default::default(),
                to: Default::default(),
                asset_id: Default::default(),
            }
        }

        pub(crate) fn create_contract_input(tx_id: TxId, output_index: u8) -> Input {
            Input::Contract {
                utxo_id: UtxoId::new(tx_id, output_index),
                balance_root: Default::default(),
                state_root: Default::default(),
                tx_pointer: Default::default(),
                contract_id: Default::default(),
            }
        }

        pub(crate) fn create_contract_output(contract_id: ContractId) -> Output {
            Output::ContractCreated {
                contract_id,
                state_root: Contract::default_state_root(),
            }
        }
    }

    #[tokio::test]
    async fn simple_insertion() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_output(create_coin_output())
                .finalize(),
        );

        txpool
            .insert_inner(tx, &db)
            .await
            .expect("Transaction should be OK, got Err");
    }

    #[tokio::test]
    async fn simple_dependency_tx1_tx2() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(tx1.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1, &db)
            .await
            .expect("Tx1 should be OK, got Err");
        txpool
            .insert_inner(tx2, &db)
            .await
            .expect("Tx2 dependent should be OK, got Err");
    }

    #[tokio::test]
    async fn faulty_t2_collided_on_contract_id_from_tx1() {
        let mut txpool = TxPool::new(Default::default());
        let mut db = MockDb::default();

        let db_tx_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        db.storage::<Coins>()
            .insert(
                &UtxoId::new(db_tx_id, 0),
                &Coin {
                    owner: Default::default(),
                    amount: Default::default(),
                    asset_id: Default::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: BlockHeight::default(),
                },
            )
            .expect("unable to insert seed coin data");

        let contract_id = ContractId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000100",
        )
        .unwrap();

        let tx = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_coin_input(db_tx_id, 0))
                .add_output(create_coin_output())
                .add_output(create_contract_output(contract_id))
                .finalize(),
        );
        let tx_faulty = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(9)
                .add_input(create_coin_input(tx.id(), 0))
                .add_output(create_contract_output(contract_id))
                .finalize(),
        );

        txpool
            .insert_inner(tx, &db)
            .await
            .expect("Tx1 should be Ok, got Err");

        let err = txpool
            .insert_inner(tx_faulty, &db)
            .await
            .expect_err("Tx2 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedCollisionContractId(id)) if id == &contract_id
        ));
    }

    #[tokio::test]
    async fn fails_to_insert_tx2_with_missing_utxo_dependency_on_faulty_tx1() {
        let mut txpool = TxPool::new(Default::default());
        let mut db = MockDb::default();

        let db_tx_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        db.storage::<Coins>()
            .insert(
                &UtxoId::new(db_tx_id, 0),
                &Coin {
                    owner: Default::default(),
                    amount: Default::default(),
                    asset_id: Default::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: BlockHeight::default(),
                },
            )
            .expect("unable to insert seed coin data");

        let contract_id = ContractId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000100",
        )
        .unwrap();
        let tx_faulty = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(db_tx_id, 0))
                .add_output(create_contract_output(contract_id))
                .finalize(),
        );
        let tx = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(tx_faulty.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx_faulty.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");

        let err = txpool
            .insert_inner(tx, &db)
            .await
            .expect_err("Tx2 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedInputUtxoIdNotExisting(id)) if id == &UtxoId::new(tx_faulty.id(), 0)
        ));
    }

    #[tokio::test]
    async fn not_inserted_known_tx() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx = Arc::new(TransactionBuilder::script(vec![], vec![]).finalize());

        txpool
            .insert_inner(tx.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");

        let err = txpool
            .insert_inner(tx, &db)
            .await
            .expect_err("Second insertion of Tx1 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedTxKnown)
        ));
    }

    #[tokio::test]
    async fn try_to_insert_tx2_missing_utxo() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let nonexistent_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000011",
        )
        .unwrap();
        let tx = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(nonexistent_id, 0))
                .finalize(),
        );

        let err = txpool
            .insert_inner(tx, &db)
            .await
            .expect_err("Tx should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedInputUtxoIdNotExisting(utxo_id)) if utxo_id == &UtxoId::new(nonexistent_id, 0)
        ));
    }

    #[tokio::test]
    async fn tx_try_to_use_spent_coin() {
        let mut txpool = TxPool::new(Default::default());
        let mut db = MockDb::default();

        let db_tx_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        db.storage::<Coins>()
            .insert(
                &UtxoId::new(db_tx_id, 0),
                &Coin {
                    owner: Default::default(),
                    amount: Default::default(),
                    asset_id: Default::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Spent,
                    block_created: BlockHeight::default(),
                },
            )
            .expect("unable to insert seed coin data");

        let tx = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(db_tx_id, 0))
                .finalize(),
        );

        let err = txpool
            .insert_inner(tx, &db)
            .await
            .expect_err("Tx should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedInputUtxoIdSpent(utxo_id)) if utxo_id == &UtxoId::new(db_tx_id, 0)
        ));
    }

    #[tokio::test]
    async fn higher_priced_tx_removes_lower_priced_tx() {
        let mut txpool = TxPool::new(Default::default());
        let mut db = MockDb::default();

        let db_tx_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        db.storage::<Coins>()
            .insert(
                &UtxoId::new(db_tx_id, 0),
                &Coin {
                    owner: Default::default(),
                    amount: Default::default(),
                    asset_id: Default::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: BlockHeight::default(),
                },
            )
            .expect("unable to insert seed coin data");

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_coin_input(db_tx_id, 0))
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .add_input(create_coin_input(db_tx_id, 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");

        let vec = txpool
            .insert_inner(tx2, &db)
            .await
            .expect("Tx2 should be Ok, got Err");
        assert_eq!(vec[0].id(), tx1.id(), "Tx1 id should be removed");
    }

    #[tokio::test]
    async fn underpriced_tx1_not_included_coin_collision() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .add_input(create_coin_input(tx1.id(), 0))
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_coin_input(tx1.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");
        txpool
            .insert_inner(tx2.clone(), &db)
            .await
            .expect("Tx2 should be Ok, got Err");

        let err = txpool
            .insert_inner(tx3.clone(), &db)
            .await
            .expect_err("Tx3 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedCollision(id, utxo_id)) if id == &tx2.id() && utxo_id == &UtxoId::new(tx1.id(), 0)
        ));
    }

    #[tokio::test]
    async fn overpriced_tx_contract_input_not_inserted() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let contract_id = ContractId::default();
        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_output(create_contract_output(contract_id))
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(11)
                .add_input(create_contract_input(
                    Default::default(),
                    Default::default(),
                ))
                .finalize(),
        );

        txpool
            .insert_inner(tx1, &db)
            .await
            .expect("Tx1 should be Ok, got err");

        let err = txpool
            .insert_inner(tx2, &db)
            .await
            .expect_err("Tx2 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedContractPricedLower(id)) if id == &contract_id
        ));
    }

    #[tokio::test]
    async fn dependent_contract_input_inserted() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let contract_id = ContractId::default();
        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_output(create_contract_output(contract_id))
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_contract_input(
                    Default::default(),
                    Default::default(),
                ))
                .finalize(),
        );

        txpool
            .insert_inner(tx1, &db)
            .await
            .expect("Tx1 should be Ok, got Err");
        txpool
            .insert_inner(tx2, &db)
            .await
            .expect("Tx2 should be Ok, got Err");
    }

    #[tokio::test]
    async fn more_priced_tx3_removes_tx1_and_dependent_tx2() {
        let mut txpool = TxPool::new(Default::default());
        let mut db = MockDb::default();

        let db_tx_id = TxId::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        db.storage::<Coins>()
            .insert(
                &UtxoId::new(db_tx_id, 0),
                &Coin {
                    owner: Default::default(),
                    amount: Default::default(),
                    asset_id: Default::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: BlockHeight::default(),
                },
            )
            .expect("unable to insert seed coin data");

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_coin_input(db_tx_id, 0))
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(9)
                .add_input(create_coin_input(tx1.id(), 0))
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .add_input(create_coin_input(db_tx_id, 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1.clone(), &db)
            .await
            .expect("Tx1 should be OK, got Err");
        txpool
            .insert_inner(tx2.clone(), &db)
            .await
            .expect("Tx2 should be OK, got Err");
        let vec = txpool
            .insert_inner(tx3.clone(), &db)
            .await
            .expect("Tx3 should be OK, got Err");
        assert_eq!(vec.len(), 2, "Tx1 and Tx2 should be removed:{:?}", vec);
        assert_eq!(vec[0].id(), tx1.id(), "Tx1 id should be removed");
        assert_eq!(vec[1].id(), tx2.id(), "Tx2 id should be removed");
    }

    #[tokio::test]
    async fn tx_limit_hit() {
        let mut txpool = TxPool::new(Config {
            max_tx: 1,
            ..Default::default()
        });
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_contract_input(tx1.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1, &db)
            .await
            .expect("Tx1 should be Ok, got Err");

        let err = txpool
            .insert_inner(tx2, &db)
            .await
            .expect_err("Tx2 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedLimitHit)
        ));
    }

    #[tokio::test]
    async fn tx_depth_hit() {
        let mut txpool = TxPool::new(Config {
            max_depth: 1,
            ..Default::default()
        });
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(tx1.id(), 0))
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .add_input(create_coin_input(tx2.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1, &db)
            .await
            .expect("Tx1 should be OK, got Err");
        txpool
            .insert_inner(tx2, &db)
            .await
            .expect("Tx1 should be OK, got Err");

        let err = txpool
            .insert_inner(tx3, &db)
            .await
            .expect_err("Tx3 should be Err, got Ok");
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedMaxDepth)
        ));
    }

    #[tokio::test]
    async fn sorted_out_tx1_2_4() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(9)
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .finalize(),
        );

        txpool
            .insert_inner(tx1.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");
        txpool
            .insert_inner(tx2.clone(), &db)
            .await
            .expect("Tx2 should be Ok, got Err");
        txpool
            .insert_inner(tx3.clone(), &db)
            .await
            .expect("Tx4 should be Ok, got Err");

        let txs = txpool.sorted_includable();

        assert_eq!(txs.len(), 3, "Should have 3 txs");
        assert_eq!(txs[0].id(), tx3.id(), "First should be tx3");
        assert_eq!(txs[1].id(), tx1.id(), "Second should be tx1");
        assert_eq!(txs[2].id(), tx2.id(), "Third should be tx2");
    }

    #[tokio::test]
    async fn find_dependent_tx1_tx2() {
        let mut txpool = TxPool::new(Default::default());
        let db = MockDb::default();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .add_input(create_coin_input(tx1.id(), 0))
                .add_output(create_coin_output())
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(9)
                .add_input(create_coin_input(tx2.id(), 0))
                .finalize(),
        );

        txpool
            .insert_inner(tx1.clone(), &db)
            .await
            .expect("Tx0 should be Ok, got Err");
        txpool
            .insert_inner(tx2.clone(), &db)
            .await
            .expect("Tx1 should be Ok, got Err");
        txpool
            .insert_inner(tx3.clone(), &db)
            .await
            .expect("Tx2 should be Ok, got Err");

        let mut seen = HashMap::new();
        txpool
            .dependency()
            .find_dependent(tx3.clone(), &mut seen, txpool.txs());

        let mut list: Vec<ArcTx> = seen.into_iter().map(|(_, tx)| tx).collect();
        // sort from high to low price
        list.sort_by_key(|tx| Reverse(tx.gas_price()));
        assert_eq!(list.len(), 2, "We should have two items");
        assert_eq!(list[0].id(), tx2.id(), "Tx2 should be first.");
        assert_eq!(list[1].id(), tx3.id(), "Tx3 should be second.");
    }

    #[tokio::test]
    async fn tx_at_least_min_gas_price_is_insertable() {
        let mut txpool = TxPool::new(Config {
            min_gas_price: 10,
            ..Default::default()
        });
        let db = MockDb::default();
        let tx = TransactionBuilder::script(vec![], vec![])
            .gas_price(10)
            .finalize();

        txpool
            .insert_inner(Arc::new(tx), &db)
            .await
            .expect("Tx should be Ok, got Err");
    }

    #[tokio::test]
    async fn tx_below_min_gas_price_is_not_insertable() {
        let mut txpool = TxPool::new(Config {
            min_gas_price: 11,
            ..Default::default()
        });
        let db = MockDb::default();
        let tx = TransactionBuilder::script(vec![], vec![])
            .gas_price(10)
            .finalize();

        let err = txpool
            .insert_inner(Arc::new(tx), &db)
            .await
            .expect_err("expected insertion failure");
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

        let mut db = MockDb::default();
        db.storage::<Messages>()
            .insert(&message.id(), &message)
            .unwrap();
        let mut txpool = TxPool::new(Default::default());

        txpool
            .insert_inner(Arc::new(tx.clone()), &db)
            .await
            .expect("should succeed");

        let tx_info = TxPool::find_one(&RwLock::new(txpool), &tx.id())
            .await
            .unwrap();
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

        let mut db = MockDb::default();
        db.storage::<Messages>()
            .insert(&message.id(), &message)
            .unwrap();
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

        let db = MockDb::default();
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
    async fn tx_rejected_from_pool_when_gas_price_is_lower_than_another_tx_with_same_message_id(
    ) {
        let message_amount = 10_000;
        let message = Message {
            amount: message_amount,
            ..Default::default()
        };

        let conflicting_message_input =
            helpers::create_message_predicate_from_message(&message);
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

        let mut db = MockDb::default();
        db.storage::<Messages>()
            .insert(&message.id(), &message)
            .unwrap();

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

        let conflicting_message_input =
            helpers::create_message_predicate_from_message(&message);
        let gas_price_high = 2u64;
        let gas_price_low = 1u64;

        // Insert a tx for the message id with a low gas amount
        let tx_low = TransactionBuilder::script(vec![], vec![])
            .gas_price(gas_price_low)
            .add_input(conflicting_message_input.clone())
            .finalize();

        let mut db = MockDb::default();
        db.storage::<Messages>()
            .insert(&message.id(), &message)
            .unwrap();

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

        let mut db = MockDb::default();
        db.storage::<Messages>()
            .insert(&message_1.id(), &message_1)
            .unwrap();
        db.storage::<Messages>()
            .insert(&message_2.id(), &message_2)
            .unwrap();
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
