use std::{
    collections::VecDeque,
    sync::Arc,
    time::{
        Duration,
        SystemTime,
    },
};

use anyhow::anyhow;
use fuel_core_types::{
    fuel_tx::{
        Bytes32,
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
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use parking_lot::RwLock;
use tokio::sync::{
    broadcast,
    Notify,
};

use crate::{
    collision_manager::basic::BasicCollisionManager,
    error::{
        Error,
        RemovedReason,
    },
    heavy_async_processing::HeavyAsyncProcessor,
    pool::Pool,
    ports::{
        AtomicView,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderTrait,
        MemoryPool as MemoryPoolTrait,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
        P2P as P2PTrait,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    storage::graph::GraphStorage,
    tx_status_stream::{
        TxStatusMessage,
        TxStatusStream,
    },
    update_sender::{
        MpscChannel,
        TxStatusChange,
    },
    verifications::perform_all_verifications,
};

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
    pub(crate) tx_status_sender: TxStatusChange,
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
    pub(crate) time_txs_submitted: Arc<RwLock<VecDeque<(SystemTime, TxId)>>>,
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
            tx_status_sender: self.tx_status_sender.clone(),
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
    ) -> Result<(), Error> {
        let current_height = *self.current_height.read();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();
        for transaction in transactions {
            self.heavy_async_processor
                .spawn({
                    let shared_state = self.clone();
                    let params = params.clone();
                    let from_peer_info = from_peer_info.clone();
                    async move {
                        let tx_clone = Arc::clone(&transaction);
                        // TODO: Return the error in the status update channel (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                        let checked_tx = match perform_all_verifications(
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
                        {
                            Ok(tx) => tx,
                            Err(Error::ConsensusValidity(_))
                            | Err(Error::MintIsDisallowed) => {
                                if let Some(from_peer_info) = from_peer_info {
                                    let _ = shared_state
                                        .p2p
                                        .notify_gossip_transaction_validity(
                                            from_peer_info,
                                            GossipsubMessageAcceptance::Reject,
                                        );
                                }
                                return;
                            }
                            Err(_) => {
                                if let Some(from_peer_info) = from_peer_info {
                                    let _ = shared_state
                                        .p2p
                                        .notify_gossip_transaction_validity(
                                            from_peer_info,
                                            GossipsubMessageAcceptance::Ignore,
                                        );
                                }
                                return;
                            }
                        };
                        let tx = Arc::new(checked_tx);

                        let result = {
                            let mut pool = shared_state.pool.write();
                            // TODO: Return the result of the insertion (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                            pool.insert(tx.clone())
                        };

                        // P2P notification
                        match (from_peer_info, result.is_ok()) {
                            (Some(from_peer_info), true) => {
                                let _ =
                                    shared_state.p2p.notify_gossip_transaction_validity(
                                        from_peer_info,
                                        GossipsubMessageAcceptance::Accept,
                                    );
                            }
                            (Some(from_peer_info), false) => {
                                let _ =
                                    shared_state.p2p.notify_gossip_transaction_validity(
                                        from_peer_info,
                                        GossipsubMessageAcceptance::Ignore,
                                    );
                            }
                            (None, _) => {
                                if let Err(e) =
                                    shared_state.p2p.broadcast_transaction(tx_clone)
                                {
                                    tracing::error!(
                                        "Failed to broadcast transaction: {}",
                                        e
                                    );
                                }
                            }
                        };

                        if let Ok(removed) = &result {
                            shared_state.new_txs_notifier.notify_waiters();
                            let submitted_time = SystemTime::now();
                            shared_state
                                .time_txs_submitted
                                .write()
                                .push_front((submitted_time, tx.id()));
                            for tx in removed {
                                shared_state.tx_status_sender.send_squeezed_out(
                                    tx.id(),
                                    Error::Removed(RemovedReason::LessWorth(tx.id())),
                                );
                            }
                            shared_state.tx_status_sender.send_submitted(
                                tx.id(),
                                Tai64::from_unix(
                                    submitted_time
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .expect("Time can't be less than UNIX EPOCH")
                                        .as_secs()
                                        as i64,
                                ),
                            );
                        }
                    }
                })
                .map_err(|_| Error::TooManyQueuedTransactions)?;
        }
        Ok(())
    }

    pub fn new_peer_subscribed(&self, peer_id: PeerId) {
        // We are not affected if there is too many queued job and we don't manage this peer.
        let _ = self.heavy_async_processor.spawn({
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
                // Verifying and insert them, not a big deal if we fail to insert them
                let _ = shared_state.insert(txs, None);
            }
        });
    }

    pub fn select_transactions(&self, max_gas: u64) -> Result<Vec<ArcPoolTx>, Error> {
        self.pool.write().extract_transactions_for_block(max_gas)
    }

    pub fn prune_old_transactions(&mut self, ttl: Duration) {
        let txs_to_remove = {
            let mut time_txs_submitted = self.time_txs_submitted.write();
            let now = SystemTime::now();
            let mut txs_to_remove = vec![];
            while let Some((time, _)) = time_txs_submitted.back() {
                let Ok(duration) = now.duration_since(*time) else {
                    tracing::error!("Failed to calculate the duration since the transaction was submitted");
                    return;
                };
                if duration < ttl {
                    break;
                }
                // SAFETY: We are removing the last element that we just checked
                txs_to_remove.push(time_txs_submitted.pop_back().unwrap().1);
            }
            txs_to_remove
        };
        {
            let mut pool = self.pool.write();
            let removed = pool
                .remove_transaction_and_dependents(txs_to_remove)
                .unwrap_or_default();
            for tx in removed {
                self.tx_status_sender
                    .send_squeezed_out(tx.id(), Error::Removed(RemovedReason::Ttl));
            }
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

    pub fn find_one(&self, tx_id: &TxId) -> Option<ArcPoolTx> {
        self.pool.read().find_one(tx_id)
    }

    pub fn find(&self, tx_ids: Vec<TxId>) -> Vec<Option<ArcPoolTx>> {
        let pool = self.pool.read();
        tx_ids
            .into_iter()
            .map(|tx_id| pool.find_one(&tx_id))
            .collect()
    }

    pub fn new_tx_notification_subscribe(&self) -> broadcast::Receiver<TxId> {
        self.tx_status_sender.new_tx_notification_sender.subscribe()
    }

    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_sender
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }

    pub fn send_complete(
        &self,
        id: Bytes32,
        block_height: &BlockHeight,
        status: TransactionStatus,
    ) {
        self.tx_status_sender.send_complete(
            id,
            block_height,
            TxStatusMessage::Status(status),
        )
    }
}
