use std::{
    collections::VecDeque,
    sync::Arc,
    time::Duration,
};

use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::{
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            PeerId,
        },
        txpool::{
            ArcPoolTx,
            PoolTransaction,
        },
    },
};
use parking_lot::RwLock;
use tokio::{
    sync::Notify,
    time::Instant,
};

use crate::{
    collision_manager::basic::BasicCollisionManager,
    error::Error,
    heavy_async_processing::HeavyAsyncProcessor,
    pool::Pool,
    ports::{
        AtomicView,
        BlockImporter as BlockImporterTrait,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderTrait,
        MemoryPool as MemoryPoolTrait,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
        P2P as P2PTrait,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    storage::{
        graph::GraphStorage,
        RemovedTransactions,
    },
    verifications::perform_all_verifications,
};

pub type InsertionResult = Result<RemovedTransactions, Error>;

pub type TxPool<PSProvider> = Arc<
    RwLock<
        Pool<
            PSProvider,
            GraphStorage,
            BasicCollisionManager<GraphStorage>,
            RatioTipGasSelection<GraphStorage>,
        >,
    >,
>;

pub struct SharedState<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> {
    pub(crate) pool: TxPool<PSProvider>,
    pub(crate) current_height: Arc<RwLock<BlockHeight>>,
    pub(crate) consensus_parameters_provider: Arc<ConsensusParamsProvider>,
    pub(crate) gas_price_provider: Arc<GasPriceProvider>,
    pub(crate) wasm_checker: Arc<WasmChecker>,
    pub(crate) memory: Arc<MemoryPool>,
    pub(crate) heavy_async_processor: Arc<HeavyAsyncProcessor>,
    pub(crate) p2p: Arc<P2P>,
    pub(crate) new_txs_notifier: Arc<Notify>,
    pub(crate) utxo_validation: bool,
    pub(crate) time_txs_submitted: Arc<RwLock<VecDeque<(Instant, TxId)>>>,
}

impl<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > Clone
    for SharedState<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
{
    fn clone(&self) -> Self {
        SharedState {
            p2p: self.p2p.clone(),
            pool: self.pool.clone(),
            current_height: self.current_height.clone(),
            consensus_parameters_provider: self.consensus_parameters_provider.clone(),
            gas_price_provider: self.gas_price_provider.clone(),
            wasm_checker: self.wasm_checker.clone(),
            memory: self.memory.clone(),
            heavy_async_processor: self.heavy_async_processor.clone(),
            new_txs_notifier: self.new_txs_notifier.clone(),
            time_txs_submitted: self.time_txs_submitted.clone(),
            utxo_validation: self.utxo_validation,
        }
    }
}

impl<
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
    SharedState<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >
where
    P2P: P2PTrait + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync + 'static,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync + 'static,
    WasmChecker: WasmCheckerTrait + Send + Sync + 'static,
    MemoryPool: MemoryPoolTrait + Send + Sync + 'static,
{
    pub fn insert(
        &self,
        transactions: Vec<Arc<Transaction>>,
        from_peer_info: Option<GossipsubMessageInfo>,
    ) -> Result<Vec<InsertionResult>, Error> {
        let current_height = *self.current_height.read();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();
        let mut results = vec![];
        for transaction in transactions {
            self.heavy_async_processor.spawn({
                let shared_state = self.clone();
                let params = params.clone();
                let from_peer_info = from_peer_info.clone();
                async move {
                    let tx_clone = Arc::clone(&transaction);
                    // TODO: Return the error in the status update channel (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                    let Ok(checked_tx) = perform_all_verifications(
                        // TODO: This should be removed if the checked transactions can work with Arc in it (see https://github.com/FuelLabs/fuel-vm/issues/831)
                        Arc::unwrap_or_clone(transaction),
                        shared_state.pool.clone(),
                        current_height,
                        &params,
                        version,
                        shared_state.gas_price_provider.as_ref(),
                        shared_state.wasm_checker.as_ref(),
                        shared_state.memory.get_memory().await,
                    )
                    .await
                    else {
                        if let Some(from_peer_info) = from_peer_info {
                            shared_state.p2p.notify_gossip_transaction_validity(
                                from_peer_info,
                                GossipsubMessageAcceptance::Reject,
                            );
                        }
                        return;
                    };
                    let tx = Arc::new(checked_tx);

                    let result = {
                        let mut pool = shared_state.pool.write();
                        // TODO: Return the result of the insertion (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                        pool.insert(tx.clone())
                    };
                    if result.is_ok() {
                        shared_state.new_txs_notifier.notify_waiters();
                        let submitted_time = Instant::now();
                        shared_state
                            .time_txs_submitted
                            .write()
                            .push_front((submitted_time, tx.id()));
                        if let Err(e) = shared_state.p2p.broadcast_transaction(tx_clone) {
                            tracing::error!("Failed to broadcast transaction: {}", e);
                        }
                    }
                    match (from_peer_info, result) {
                        (Some(from_peer_info), Ok(_)) => {
                            shared_state.p2p.notify_gossip_transaction_validity(
                                from_peer_info,
                                GossipsubMessageAcceptance::Accept,
                            );
                        }
                        (Some(from_peer_info), Err(_)) => {
                            shared_state.p2p.notify_gossip_transaction_validity(
                                from_peer_info,
                                GossipsubMessageAcceptance::Ignore,
                            );
                        }
                        (None, _) => {}
                    }
                }
            });
        }
        Ok(results)
    }

    pub fn new_peer_subscribed(&self, peer_id: PeerId) {
        self.heavy_async_processor.spawn({
            let shared_state = self.clone();
            async move {
                let peer_tx_ids = shared_state
                    .p2p
                    .request_tx_ids(peer_id.clone())
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            "Failed to gather tx ids from peer {}: {}",
                            &peer_id,
                            e
                        );
                    })
                    .unwrap_or_default();
                if peer_tx_ids.is_empty() {
                    return;
                }
                let tx_ids_to_ask = shared_state.filter_existing_tx_ids(peer_tx_ids);
                if tx_ids_to_ask.is_empty() {
                    return;
                }
                let txs: Vec<Arc<Transaction>> = shared_state
                    .p2p
                    .request_txs(peer_id.clone(), tx_ids_to_ask)
                    .await
                    .inspect_err(|e| {
                        tracing::error!(
                            "Failed to gather tx ids from peer {}: {}",
                            &peer_id,
                            e
                        );
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .map(Arc::new)
                    .collect();
                if txs.is_empty() {
                    return;
                }
                // Verifying them
                shared_state.insert(txs, None);
            }
        });
    }

    pub fn select_transactions(&self, max_gas: u64) -> Result<Vec<ArcPoolTx>, Error> {
        self.pool.write().extract_transactions_for_block(max_gas)
    }

    pub fn prune_old_transactions(&mut self, ttl: Duration) {
        let txs_to_remove = {
            let mut time_txs_submitted = self.time_txs_submitted.write();
            let now = Instant::now();
            let mut txs_to_remove = vec![];
            while let Some((time, _)) = time_txs_submitted.back() {
                if now.duration_since(*time) < ttl {
                    break;
                }
                // SAFETY: We are removing the last element that we just checked
                txs_to_remove.push(time_txs_submitted.pop_back().unwrap().1);
            }
            txs_to_remove
        };
        {
            let mut pool = self.pool.write();
            pool.remove_transaction_and_dependents(txs_to_remove);
        }
    }

    pub fn get_new_txs_notifier(&self) -> Arc<Notify> {
        self.new_txs_notifier.clone()
    }

    pub fn filter_existing_tx_ids(&self, tx_ids: Vec<TxId>) -> Vec<TxId> {
        let pool = self.pool.read();
        tx_ids
            .into_iter()
            .filter(|tx_id| !pool.contains(tx_id))
            .collect()
    }

    pub fn get_tx_ids(&self, max_txs: usize) -> Vec<TxId> {
        self.pool
            .read()
            .iter_tx_ids()
            .take(max_txs)
            .copied()
            .collect()
    }

    pub fn get_txs(&self, tx_ids: Vec<TxId>) -> Vec<Option<ArcPoolTx>> {
        let pool = self.pool.read();
        tx_ids
            .into_iter()
            .map(|tx_id| pool.find_one(&tx_id))
            .collect()
    }
}
