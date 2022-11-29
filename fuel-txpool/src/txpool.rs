use crate::{
    containers::{
        dependency::Dependency,
        price_sort::PriceSort,
    },
    service::TxStatusChange,
    types::*,
    Config,
    Error,
};
use anyhow::anyhow;
use fuel_core_interfaces::{
    common::fuel_tx::{
        Chargeable,
        CheckedTransaction,
        IntoChecked,
        Transaction,
        UniqueIdentifier,
    },
    model::{
        ArcPoolTx,
        FuelBlock,
        TxInfo,
    },
    txpool::{
        InsertionResult,
        TxPoolDb,
    },
};
use fuel_metrics::txpool_metrics::TXPOOL_METRICS;
use std::{
    cmp::Reverse,
    collections::HashMap,
    ops::Deref,
    sync::Arc,
};
use tokio::sync::RwLock;

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
            by_dependency: Dependency::new(max_depth, config.utxo_validation),
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
    async fn insert_inner(
        &mut self,
        // TODO: Pass `&Transaction`
        tx: Arc<Transaction>,
        db: &dyn TxPoolDb,
    ) -> anyhow::Result<InsertionResult> {
        let current_height = db.current_block_height()?;

        if tx.is_mint() {
            return Err(Error::NotSupportedTransactionType.into())
        }

        // verify gas price is at least the minimum
        self.verify_tx_min_gas_price(&tx)?;

        let tx: CheckedTransaction = if self.config.utxo_validation {
            tx.deref()
                .clone()
                .into_checked(
                    current_height.into(),
                    &self.config.chain_config.transaction_parameters,
                )?
                .into()
        } else {
            tx.deref()
                .clone()
                .into_checked_basic(
                    current_height.into(),
                    &self.config.chain_config.transaction_parameters,
                )?
                .into()
        };

        let tx = Arc::new(match tx {
            CheckedTransaction::Script(script) => PoolTransaction::Script(script),
            CheckedTransaction::Create(create) => PoolTransaction::Create(create),
            CheckedTransaction::Mint(_) => unreachable!(),
        });

        if !tx.is_computed() {
            return Err(Error::NoMetadata.into())
        }

        // verify max gas is less than block limit
        if tx.max_gas() > self.config.chain_config.block_gas_limit {
            return Err(Error::NotInsertedMaxGasLimit {
                tx_gas: tx.max_gas(),
                block_limit: self.config.chain_config.block_gas_limit,
            }
            .into())
        }

        // verify predicates
        if !tx.check_predicates(self.config.chain_config.transaction_parameters) {
            return Err(anyhow!("transaction predicate verification failed"))
        }

        if self.by_hash.contains_key(&tx.id()) {
            return Err(Error::NotInsertedTxKnown.into())
        }

        let mut max_limit_hit = false;
        // check if we are hitting limit of pool
        if self.by_hash.len() >= self.config.max_tx {
            max_limit_hit = true;
            // limit is hit, check if we can push out lowest priced tx
            let lowest_price = self.by_gas_price.lowest_price();
            if lowest_price >= tx.price() {
                return Err(Error::NotInsertedLimitHit.into())
            }
        }
        if self.config.metrics {
            TXPOOL_METRICS
                .gas_price_histogram
                .observe(tx.price() as f64);

            TXPOOL_METRICS
                .tx_size_histogram
                .observe(tx.metered_bytes_size() as f64);
        }
        // check and insert dependency
        let rem = self.by_dependency.insert(&self.by_hash, db, &tx).await?;
        self.by_hash.insert(tx.id(), TxInfo::new(tx.clone()));
        self.by_gas_price.insert(&tx);

        // if some transaction were removed so we don't need to check limit
        let removed = if rem.is_empty() {
            if max_limit_hit {
                // remove last tx from sort
                let rem_tx = self.by_gas_price.last().unwrap(); // safe to unwrap limit is hit
                self.remove_inner(&rem_tx);
                vec![rem_tx]
            } else {
                Vec::new()
            }
        } else {
            // remove ret from by_hash and from by_price
            for rem in rem.iter() {
                self.by_hash
                    .remove(&rem.id())
                    .expect("Expect to hash of tx to be present");
                self.by_gas_price.remove(rem);
            }

            rem
        };

        Ok(InsertionResult {
            inserted: tx,
            removed,
        })
    }

    /// Return all sorted transactions that are includable in next block.
    pub fn sorted_includable(&self) -> Vec<ArcPoolTx> {
        self.by_gas_price
            .sort
            .iter()
            .rev()
            .map(|(_, tx)| tx.clone())
            .collect()
    }

    pub fn remove_inner(&mut self, tx: &ArcPoolTx) -> Vec<ArcPoolTx> {
        self.remove_by_tx_id(&tx.id())
    }

    /// remove transaction from pool needed on user demand. Low priority
    pub fn remove_by_tx_id(&mut self, tx_id: &TxId) -> Vec<ArcPoolTx> {
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
        let price = match tx {
            Transaction::Script(script) => script.price(),
            Transaction::Create(create) => create.price(),
            Transaction::Mint(_) => unreachable!(),
        };
        if self.config.metrics {
            // Gas Price metrics are recorded here to avoid double matching for
            // every single transaction, but also means metrics aren't collected on gas
            // price if there is no minimum gas price
            TXPOOL_METRICS.gas_price_histogram.observe(price as f64);
        }
        if price < self.config.min_gas_price {
            return Err(Error::NotInsertedGasPriceTooLow)
        }
        Ok(())
    }

    /// Import a set of transactions from network gossip or GraphQL endpoints.
    pub async fn insert(
        txpool: &RwLock<Self>,
        db: &dyn TxPoolDb,
        tx_status_sender: &TxStatusChange,
        txs: &[Arc<Transaction>],
    ) -> Vec<anyhow::Result<InsertionResult>> {
        // Check if that data is okay (witness match input/output, and if recovered signatures ara valid).
        // should be done before transaction comes to txpool, or before it enters RwLocked region.
        let mut res = Vec::new();
        for tx in txs.iter() {
            let mut pool = txpool.write().await;
            res.push(pool.insert_inner(tx.clone(), db).await)
        }
        // announce to subscribers
        for ret in res.iter() {
            match ret {
                Ok(InsertionResult { removed, inserted }) => {
                    for removed in removed {
                        // small todo there is possibility to have removal reason (ReplacedByHigherGas, DependencyRemoved)
                        // but for now it is okay to just use Error::Removed.
                        tx_status_sender.send_squeezed_out(removed.id(), Error::Removed);
                    }
                    tx_status_sender.send_submitted(inserted.id());
                }
                Err(_) => {
                    // @dev should not broadcast tx if error occurred
                }
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
    pub async fn find_dependent(
        txpool: &RwLock<Self>,
        hashes: &[TxId],
    ) -> Vec<ArcPoolTx> {
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
        let mut list: Vec<ArcPoolTx> = seen.into_iter().map(|(_, tx)| tx).collect();
        // sort from high to low price
        list.sort_by_key(|tx| Reverse(tx.price()));

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

    /// The number of pending transaction in the pool.
    pub async fn pending_number(txpool: &RwLock<Self>) -> usize {
        let pool = txpool.read().await;
        pool.by_hash.len()
    }

    /// The amount of gas in all includable transactions combined
    pub async fn consumable_gas(txpool: &RwLock<Self>) -> u64 {
        let pool = txpool.read().await;
        pool.by_hash.values().map(|tx| tx.limit()).sum()
    }

    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    pub async fn includable(txpool: &RwLock<Self>) -> Vec<ArcPoolTx> {
        let pool = txpool.read().await;
        pool.sorted_includable()
    }

    /// When block is updated we need to receive all spend outputs and remove them from txpool.
    pub async fn block_update(
        txpool: &RwLock<Self>,
        tx_status_sender: &TxStatusChange,
        block: Arc<FuelBlock>,
        // spend_outputs: [Input], added_outputs: [AddedOutputs]
    ) {
        let mut guard = txpool.write().await;
        // TODO https://github.com/FuelLabs/fuel-core/issues/465

        for tx in block.transactions() {
            tx_status_sender.send_complete(tx.id());
            let _removed = guard.remove_by_tx_id(&tx.id());
        }
    }

    /// remove transaction from pool needed on user demand. Low priority
    pub async fn remove(
        txpool: &RwLock<Self>,
        tx_status_sender: &TxStatusChange,
        tx_ids: &[TxId],
    ) -> Vec<ArcPoolTx> {
        let mut removed = Vec::new();
        for tx_id in tx_ids {
            let rem = { txpool.write().await.remove_by_tx_id(tx_id) };
            tx_status_sender.send_squeezed_out(*tx_id, Error::Removed);
            removed.extend(rem.into_iter());
        }
        removed
    }
}

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;
