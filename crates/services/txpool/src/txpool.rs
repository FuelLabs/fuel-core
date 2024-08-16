use crate::{
    containers::{
        dependency::Dependency,
        time_sort::TimeSort,
        tip_per_gas_sort::RatioGasTipSort,
    },
    ports::{
        TxPoolDb,
        WasmChecker as WasmCheckerConstraint,
    },
    service::TxStatusChange,
    types::*,
    Config,
    Error,
    TxInfo,
};
use fuel_core_types::{
    fuel_tx::{
        field::UpgradePurpose as _,
        Transaction,
        UpgradePurpose,
    },
    fuel_types::BlockHeight,
    fuel_vm::checked_transaction::{
        CheckPredicates,
        Checked,
        CheckedTransaction,
        Checks,
        IntoChecked,
    },
    services::txpool::{
        ArcPoolTx,
        InsertionResult,
    },
    tai64::Tai64,
};
use num_rational::Ratio;

use crate::ports::{
    GasPriceProvider,
    MemoryPool,
};
use fuel_core_metrics::txpool_metrics::txpool_metrics;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::header::ConsensusParametersVersion,
    fuel_tx::{
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        ConsensusParameters,
        Input,
    },
    fuel_vm::{
        checked_transaction::CheckPredicateParams,
        interpreter::Memory,
    },
    services::executor::TransactionExecutionStatus,
};
use std::{
    collections::HashMap,
    ops::Deref,
    sync::Arc,
};

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct TxPool<ViewProvider, WasmChecker> {
    by_hash: HashMap<TxId, TxInfo>,
    by_ratio_gas_tip: RatioGasTipSort,
    by_time: TimeSort,
    by_dependency: Dependency,
    config: Config,
    database: ViewProvider,
    wasm_checker: WasmChecker,
}

impl<ViewProvider, WasmChecker> TxPool<ViewProvider, WasmChecker> {
    pub fn new(
        config: Config,
        database: ViewProvider,
        wasm_checker: WasmChecker,
    ) -> Self {
        let max_depth = config.max_depth;

        Self {
            by_hash: HashMap::new(),
            by_ratio_gas_tip: RatioGasTipSort::default(),
            by_time: TimeSort::default(),
            by_dependency: Dependency::new(max_depth, config.utxo_validation),
            config,
            database,
            wasm_checker,
        }
    }

    #[cfg(test)]
    pub fn config(&self) -> &Config {
        &self.config
    }

    #[cfg(test)]
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn txs(&self) -> &HashMap<TxId, TxInfo> {
        &self.by_hash
    }

    pub fn dependency(&self) -> &Dependency {
        &self.by_dependency
    }

    /// Return all sorted transactions that are includable in next block.
    pub fn sorted_includable(&self) -> impl Iterator<Item = ArcPoolTx> + '_ {
        self.by_ratio_gas_tip
            .sort
            .iter()
            .rev()
            .map(|(_, tx)| tx.clone())
    }

    pub fn remove_inner(&mut self, tx: &ArcPoolTx) -> Vec<ArcPoolTx> {
        self.remove_by_tx_id(&tx.id())
    }

    /// remove transaction from pool needed on user demand. Low priority
    // TODO: Seems this function should be recursive
    pub fn remove_by_tx_id(&mut self, tx_id: &TxId) -> Vec<ArcPoolTx> {
        if let Some(tx) = self.remove_tx(tx_id) {
            let removed = self
                .by_dependency
                .recursively_remove_all_dependencies(&self.by_hash, tx.tx().clone());
            for remove in removed.iter() {
                self.remove_tx(&remove.id());
            }
            return removed
        }
        Vec::new()
    }

    fn remove_tx(&mut self, tx_id: &TxId) -> Option<TxInfo> {
        let info = self.by_hash.remove(tx_id);
        if let Some(info) = &info {
            self.by_time.remove(info);
            self.by_ratio_gas_tip.remove(info);
        }

        info
    }

    /// Removes transaction from `TxPool` with assumption that it is committed into the blockchain.
    // TODO: Don't remove recursively dependent transactions on block commit.
    //  The same logic should be fixed in the `select_transactions`.
    //  This method is used during `select_transactions`, so we need to handle the case
    //  when transaction was skipped during block execution(`ExecutionResult.skipped_transaction`).
    pub fn remove_committed_tx(&mut self, tx_id: &TxId) -> Vec<ArcPoolTx> {
        self.remove_by_tx_id(tx_id)
    }

    /// find all tx by its hash
    pub fn find(&self, hashes: &[TxId]) -> Vec<Option<TxInfo>> {
        let mut res = Vec::with_capacity(hashes.len());
        for hash in hashes {
            res.push(self.txs().get(hash).cloned());
        }
        res
    }

    pub fn find_one(&self, hash: &TxId) -> Option<TxInfo> {
        self.txs().get(hash).cloned()
    }

    /// The number of pending transaction in the pool.
    pub fn pending_number(&self) -> usize {
        self.by_hash.len()
    }

    /// The amount of gas in all includable transactions combined
    pub fn consumable_gas(&self) -> u64 {
        self.by_hash.values().map(|tx| tx.max_gas()).sum()
    }

    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    pub fn includable(&mut self) -> impl Iterator<Item = ArcPoolTx> + '_ {
        self.sorted_includable()
    }

    /// When block is updated we need to receive all spend outputs and remove them from txpool.
    pub fn block_update(
        &mut self,
        tx_status: &[TransactionExecutionStatus],
        // spend_outputs: [Input], added_outputs: [AddedOutputs]
    ) {
        for status in tx_status {
            let tx_id = status.id;
            self.remove_committed_tx(&tx_id);
        }
    }

    /// remove transaction from pool needed on user demand. Low priority
    pub fn remove(
        &mut self,
        tx_status_sender: &TxStatusChange,
        tx_ids: Vec<(TxId, String)>,
    ) -> Vec<ArcPoolTx> {
        let mut removed = Vec::new();
        for (tx_id, reason) in tx_ids.into_iter() {
            let rem = self.remove_by_tx_id(&tx_id);
            tx_status_sender.send_squeezed_out(tx_id, Error::SqueezedOut(reason.clone()));
            for dependent_tx in rem.iter() {
                if tx_id != dependent_tx.id() {
                    tx_status_sender.send_squeezed_out(
                        dependent_tx.id(),
                        Error::SqueezedOut(
                            format!("Parent transaction with {tx_id}, was removed because of the {reason}")
                        )
                    );
                }
            }
            removed.extend(rem.into_iter());
        }
        removed
    }

    /// Remove all old transactions from the pool.
    pub fn prune_old_txs(&mut self) -> Vec<ArcPoolTx> {
        let Some(deadline) =
            tokio::time::Instant::now().checked_sub(self.config.transaction_ttl)
        else {
            // TTL is so big that we don't need to prune any transactions
            return vec![]
        };

        let mut result = vec![];

        while let Some((oldest_time, oldest_tx)) = self.by_time.lowest() {
            let oldest_tx = oldest_tx.clone();
            if oldest_time.created() <= &deadline {
                let removed = self.remove_inner(&oldest_tx);
                result.extend(removed.into_iter());
            } else {
                break
            }
        }

        result
    }

    fn check_blacklisting(&self, tx: &PoolTransaction) -> Result<(), Error> {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, owner, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, owner, .. }) => {
                    if self.config.blacklist.contains_coin(utxo_id) {
                        return Err(Error::BlacklistedUTXO(*utxo_id))
                    }
                    if self.config.blacklist.contains_address(owner) {
                        return Err(Error::BlacklistedOwner(*owner))
                    }
                }
                Input::Contract(contract) => {
                    if self
                        .config
                        .blacklist
                        .contains_contract(&contract.contract_id)
                    {
                        return Err(Error::BlacklistedContract(contract.contract_id))
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageCoinPredicate(MessageCoinPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataSigned(MessageDataSigned {
                    nonce,
                    sender,
                    recipient,
                    ..
                })
                | Input::MessageDataPredicate(MessageDataPredicate {
                    nonce,
                    sender,
                    recipient,
                    ..
                }) => {
                    if self.config.blacklist.contains_message(nonce) {
                        return Err(Error::BlacklistedMessage(*nonce))
                    }
                    if self.config.blacklist.contains_address(sender) {
                        return Err(Error::BlacklistedOwner(*sender))
                    }
                    if self.config.blacklist.contains_address(recipient) {
                        return Err(Error::BlacklistedOwner(*recipient))
                    }
                }
            }
        }

        Ok(())
    }
}

impl<ViewProvider, View, WasmChecker> TxPool<ViewProvider, WasmChecker>
where
    ViewProvider: AtomicView<LatestView = View>,
    View: TxPoolDb,
    WasmChecker: WasmCheckerConstraint + Send + Sync,
{
    #[cfg(test)]
    fn insert_single(
        &mut self,
        tx: Checked<Transaction>,
    ) -> Result<InsertionResult, Error> {
        let view = self.database.latest_view().unwrap();
        self.insert_inner(tx, ConsensusParametersVersion::MIN, &view)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(tx_id = %tx.id()), ret, err)]
    // this is atomic operation. Return removed(pushed out/replaced) transactions
    fn insert_inner(
        &mut self,
        tx: Checked<Transaction>,
        version: ConsensusParametersVersion,
        view: &View,
    ) -> Result<InsertionResult, Error> {
        let tx: CheckedTransaction = tx.into();

        let tx = Arc::new(match tx {
            CheckedTransaction::Script(tx) => PoolTransaction::Script(tx, version),
            CheckedTransaction::Create(tx) => PoolTransaction::Create(tx, version),
            CheckedTransaction::Mint(_) => return Err(Error::MintIsDisallowed),
            CheckedTransaction::Upgrade(tx) => PoolTransaction::Upgrade(tx, version),
            CheckedTransaction::Upload(tx) => PoolTransaction::Upload(tx, version),
            CheckedTransaction::Blob(tx) => PoolTransaction::Blob(tx, version),
        });

        self.check_blacklisting(tx.as_ref())?;

        if !tx.is_computed() {
            return Err(Error::NoMetadata)
        }

        if self.by_hash.contains_key(&tx.id()) {
            return Err(Error::NotInsertedTxKnown)
        }

        // If upgrading transition function, at least check that it is valid wasm
        if let PoolTransaction::Upgrade(upgrade, _) = tx.as_ref() {
            if let UpgradePurpose::StateTransition { root } =
                upgrade.transaction().upgrade_purpose()
            {
                self.wasm_checker.validate_uploaded_wasm(root)?;
            }
        }

        let mut max_limit_hit = false;
        // check if we are hitting limit of pool
        if self.by_hash.len() >= self.config.max_tx {
            max_limit_hit = true;
            // limit is hit, check if we can push out lowest priced tx
            let lowest_ratio = self.by_ratio_gas_tip.lowest_value().unwrap_or_default();
            if lowest_ratio >= Ratio::new(tx.tip(), tx.max_gas()) {
                return Err(Error::NotInsertedLimitHit)
            }
        }
        if self.config.metrics {
            txpool_metrics()
                .tx_size_histogram
                .observe(tx.metered_bytes_size() as f64);
        }
        // check and insert dependency
        let rem = self.by_dependency.insert(&self.by_hash, view, &tx)?;
        let info = TxInfo::new(tx.clone());
        let submitted_time = info.submitted_time();
        self.by_ratio_gas_tip.insert(&info);
        self.by_time.insert(&info);
        self.by_hash.insert(tx.id(), info);

        // if some transaction were removed so we don't need to check limit
        let removed = if rem.is_empty() {
            if max_limit_hit {
                // remove last tx from sort
                let rem_tx = self.by_ratio_gas_tip.lowest_tx().unwrap(); // safe to unwrap limit is hit
                self.remove_inner(&rem_tx);
                vec![rem_tx]
            } else {
                Vec::new()
            }
        } else {
            // remove ret from by_hash and from by_price
            for rem in rem.iter() {
                self.remove_tx(&rem.id());
            }

            rem
        };

        Ok(InsertionResult {
            inserted: tx,
            submitted_time,
            removed,
        })
    }

    #[tracing::instrument(level = "info", skip_all)]
    /// Import a set of transactions from network gossip or GraphQL endpoints.
    pub fn insert(
        &mut self,
        tx_status_sender: &TxStatusChange,
        version: ConsensusParametersVersion,
        txs: Vec<Checked<Transaction>>,
    ) -> Vec<Result<InsertionResult, Error>> {
        // Check if that data is okay (witness match input/output, and if recovered signatures ara valid).
        // should be done before transaction comes to txpool, or before it enters RwLocked region.
        let mut res = Vec::new();
        let view = match self.database.latest_view() {
            Ok(view) => view,
            Err(e) => return vec![Err(Error::Other(e.to_string()))],
        };

        for tx in txs.into_iter() {
            res.push(self.insert_inner(tx, version, &view));
        }

        // announce to subscribers
        for ret in res.iter() {
            match ret {
                Ok(InsertionResult {
                    removed,
                    inserted,
                    submitted_time,
                }) => {
                    for removed in removed {
                        // small todo there is possibility to have removal reason (ReplacedByHigherGas, DependencyRemoved)
                        // but for now it is okay to just use Error::Removed.
                        tx_status_sender.send_squeezed_out(removed.id(), Error::Removed);
                    }
                    tx_status_sender.send_submitted(
                        inserted.id(),
                        Tai64::from_unix(submitted_time.as_secs() as i64),
                    );
                }
                Err(_) => {
                    // @dev should not broadcast tx if error occurred
                }
            }
        }
        res
    }
}

pub async fn check_transactions<Provider, MP>(
    txs: &[Arc<Transaction>],
    current_height: BlockHeight,
    utxp_validation: bool,
    consensus_params: &ConsensusParameters,
    gas_price_provider: &Provider,
    memory_pool: Arc<MP>,
) -> Vec<Result<Checked<Transaction>, Error>>
where
    Provider: GasPriceProvider,
    MP: MemoryPool,
{
    let mut checked_txs = Vec::with_capacity(txs.len());

    for tx in txs.iter() {
        checked_txs.push(
            check_single_tx(
                tx.deref().clone(),
                current_height,
                utxp_validation,
                consensus_params,
                gas_price_provider,
                memory_pool.get_memory().await,
            )
            .await,
        );
    }

    checked_txs
}

pub async fn check_single_tx<GasPrice, M>(
    tx: Transaction,
    current_height: BlockHeight,
    utxo_validation: bool,
    consensus_params: &ConsensusParameters,
    gas_price_provider: &GasPrice,
    memory: M,
) -> Result<Checked<Transaction>, Error>
where
    GasPrice: GasPriceProvider,
    M: Memory + Send + Sync + 'static,
{
    if tx.is_mint() {
        return Err(Error::NotSupportedTransactionType)
    }

    let tx: Checked<Transaction> = if utxo_validation {
        let tx = tx
            .into_checked_basic(current_height, consensus_params)?
            .check_signatures(&consensus_params.chain_id())?;

        let parameters = CheckPredicateParams::from(consensus_params);
        let tx =
            tokio_rayon::spawn_fifo(move || tx.check_predicates(&parameters, memory))
                .await?;

        debug_assert!(tx.checks().contains(Checks::all()));

        tx
    } else {
        tx.into_checked_basic(current_height, consensus_params)?
    };

    let gas_price = gas_price_provider.next_gas_price().await?;

    let tx = verify_tx_min_gas_price(tx, consensus_params, gas_price)?;

    Ok(tx)
}

fn verify_tx_min_gas_price(
    tx: Checked<Transaction>,
    consensus_params: &ConsensusParameters,
    gas_price: GasPrice,
) -> Result<Checked<Transaction>, Error> {
    let tx: CheckedTransaction = tx.into();
    let gas_costs = consensus_params.gas_costs();
    let fee_parameters = consensus_params.fee_params();
    let read = match tx {
        CheckedTransaction::Script(script) => {
            let ready = script.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Script(checked)
        }
        CheckedTransaction::Create(create) => {
            let ready = create.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Create(checked)
        }
        CheckedTransaction::Upgrade(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Upgrade(checked)
        }
        CheckedTransaction::Upload(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Upload(checked)
        }
        CheckedTransaction::Blob(tx) => {
            let ready = tx.into_ready(gas_price, gas_costs, fee_parameters)?;
            let (_, checked) = ready.decompose();
            CheckedTransaction::Blob(checked)
        }
        CheckedTransaction::Mint(_) => return Err(Error::MintIsDisallowed),
    };
    Ok(read.into())
}
