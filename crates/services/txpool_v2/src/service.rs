use std::{
    collections::VecDeque,
    sync::Arc,
    time::SystemTime,
};

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipData,
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            PeerId,
            TransactionGossipData,
        },
        txpool::{
            ArcPoolTx,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use futures::StreamExt;
use parking_lot::RwLock;
use tokio::{
    sync::{
        mpsc,
        oneshot,
        Notify,
    },
    time::MissedTickBehavior,
};

use crate::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::{
        Error,
        RemovedReason,
    },
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
    selection_algorithms::{
        ratio_tip_gas::RatioTipGasSelection,
        Constraints,
    },
    shared_state::SharedState,
    storage::{
        graph::{
            GraphConfig,
            GraphStorage,
        },
        Storage,
    },
    update_sender::TxStatusChange,
    verifications::perform_all_verifications,
};

/// Structure returned to others modules containing the transaction and
/// some useful infos
#[derive(Debug)]
pub struct TxInfo {
    /// The transaction
    tx: ArcPoolTx,
    /// The creation instant of the transaction
    creation_instant: SystemTime,
}

impl TxInfo {
    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn creation_instant(&self) -> &SystemTime {
        &self.creation_instant
    }
}

impl From<TxInfo> for TransactionStatus {
    fn from(tx_info: TxInfo) -> Self {
        Self::Submitted {
            time: Tai64::from_unix(
                tx_info
                    .creation_instant
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Time can't be less than UNIX EPOCH")
                    .as_secs() as i64,
            ),
        }
    }
}

pub type TxPool<PSProvider, GasPriceProvider, ConsensusParametersProvider> = Arc<
    RwLock<
        Pool<
            PSProvider,
            GraphStorage,
            <GraphStorage as Storage>::StorageIndex,
            BasicCollisionManager<<GraphStorage as Storage>::StorageIndex>,
            RatioTipGasSelection<
                GraphStorage,
                GasPriceProvider,
                ConsensusParametersProvider,
            >,
        >,
    >,
>;

pub type Service<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> = ServiceRunner<
    Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >,
>;

pub struct SelectTransactionsRequest {
    pub constraints: Constraints,
    pub response_channel: oneshot::Sender<Result<Vec<ArcPoolTx>, Error>>,
}

pub enum WritePoolRequest {
    InsertTxs {
        transactions: Vec<Arc<Transaction>>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    },
    RemoveCoinDependents {
        tx_id: TxId,
        response_channel: oneshot::Sender<Vec<ArcPoolTx>>,
    },
}

pub enum ReadPoolRequest {
    GetTxIds {
        max_txs: usize,
        response_channel: oneshot::Sender<Vec<TxId>>,
    },
    GetTxs {
        tx_ids: Vec<TxId>,
        response_channel: oneshot::Sender<Vec<Option<TxInfo>>>,
    },
}

pub struct Task<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
> {
    tx_from_p2p_stream: BoxStream<TransactionGossipData>,
    new_peers_subscribed_stream: BoxStream<PeerId>,
    imported_block_results_stream: BoxStream<SharedImportResult>,
    select_transactions_requests_receiver: mpsc::Receiver<SelectTransactionsRequest>,
    write_pool_requests_sender: mpsc::Sender<WritePoolRequest>,
    write_pool_requests_receiver: mpsc::Receiver<WritePoolRequest>,
    read_pool_requests_receiver: mpsc::Receiver<ReadPoolRequest>,
    pool: TxPool<PSProvider, GasPriceProvider, ConsensusParamsProvider>,
    current_height: Arc<RwLock<BlockHeight>>,
    consensus_parameters_provider: Arc<ConsensusParamsProvider>,
    gas_price_provider: Arc<GasPriceProvider>,
    wasm_checker: Arc<WasmChecker>,
    memory: Arc<MemoryPool>,
    heavy_verif_insert_processor: Arc<HeavyAsyncProcessor>,
    heavy_p2p_sync_processor: Arc<HeavyAsyncProcessor>,
    p2p: Arc<P2P>,
    new_txs_notifier: Arc<Notify>,
    time_txs_submitted: Arc<RwLock<VecDeque<(SystemTime, TxId)>>>,
    ttl_timer: tokio::time::Interval,
    txs_ttl: tokio::time::Duration,
    tx_status_sender: TxStatusChange,
    shared_state: SharedState,
}

#[async_trait::async_trait]
impl<
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableService
    for Task<
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
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState;

    type Task = Task<
        P2P,
        PSProvider,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    >;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<
        P2P,
        PSProvider,
        PSView,
        ConsensusParamsProvider,
        GasPriceProvider,
        WasmChecker,
        MemoryPool,
    > RunnableTask
    for Task<
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
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            block_result = self.imported_block_results_stream.next() => {
                if let Some(result) = block_result {
                    self.manage_imported_block(result)?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            select_transaction_request = self.select_transactions_requests_receiver.recv() => {
                if let Some(select_transaction_request) = select_transaction_request {
                    self.manage_select_transactions(select_transaction_request).await?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            write_pool_request = self.write_pool_requests_receiver.recv() => {
                if let Some(write_pool_request) = write_pool_request {
                    match write_pool_request {
                        WritePoolRequest::InsertTxs {
                            transactions,
                            from_peer_info,
                            response_channel,
                        } => {
                            self.manage_tx_insertions(transactions, from_peer_info, response_channel)?;
                        }
                        WritePoolRequest::RemoveCoinDependents {
                            tx_id,
                            response_channel,
                        } => {
                            self.manage_remove_coin_dependents(tx_id, response_channel)?;
                        }
                    }

                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            tx_from_p2p = self.tx_from_p2p_stream.next() => {
                if let Some(GossipData { data: Some(tx), message_id, peer_id }) = tx_from_p2p {
                    self.manage_tx_from_p2p(tx, message_id, peer_id).await?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_peer_subscribed = self.new_peers_subscribed_stream.next() => {
                if let Some(peer_id) = new_peer_subscribed {
                    self.manage_new_peer_subscribed(peer_id);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            read_pool_request = self.read_pool_requests_receiver.recv() => {
                if let Some(read_pool_request) = read_pool_request {
                    self.manage_find_transactions(read_pool_request)?;
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            _ = self.ttl_timer.tick() => {
                self.manage_prune_old_transactions();
                should_continue = true;
            }

        }

        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
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
    Task<
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
    fn manage_imported_block(
        &mut self,
        result: SharedImportResult,
    ) -> anyhow::Result<()> {
        {
            let mut tx_pool = self.pool.write();
            tx_pool
                .remove_transaction(result.tx_status.iter().map(|s| s.id).collect())
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        let new_height = *result.sealed_block.entity.header().height();
        {
            let mut block_height = self.current_height.write();
            *block_height = new_height;
        }
        Ok(())
    }

    async fn manage_select_transactions(
        &self,
        SelectTransactionsRequest {
            constraints,
            response_channel,
        }: SelectTransactionsRequest,
    ) -> anyhow::Result<()> {
        let result = self
            .pool
            .write()
            .extract_transactions_for_block(constraints);
        response_channel
            .send(result)
            .map_err(|_| anyhow::anyhow!("Failed to send the result"))
    }

    fn manage_tx_insertions(
        &self,
        transactions: Vec<Arc<Transaction>>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    ) -> anyhow::Result<()> {
        let current_height = *self.current_height.read();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();
        for transaction in transactions {
            let result = self.heavy_verif_insert_processor.spawn({
                let params = params.clone();
                let from_peer_info = from_peer_info.clone();
                let pool = self.pool.clone();
                let gas_price_provider = self.gas_price_provider.clone();
                let wasm_checker = self.wasm_checker.clone();
                let memory = self.memory.clone();
                let p2p = self.p2p.clone();
                let new_txs_notifier = self.new_txs_notifier.clone();
                let time_txs_submitted = self.time_txs_submitted.clone();
                let tx_status_sender = self.tx_status_sender.clone();
                async move {
                    let tx_clone = Arc::clone(&transaction);
                    // TODO: Return the error in the status update channel (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                    let checked_tx = match perform_all_verifications(
                        // TODO: This should be removed if the checked transactions can work with Arc in it (see https://github.com/FuelLabs/fuel-vm/issues/831)
                        Arc::unwrap_or_clone(transaction),
                        &pool,
                        current_height,
                        &params,
                        version,
                        gas_price_provider.as_ref(),
                        wasm_checker.as_ref(),
                        memory.get_memory().await,
                    )
                    .await
                    {
                        Ok(tx) => tx,
                        Err(Error::ConsensusValidity(_))
                        | Err(Error::MintIsDisallowed) => {
                            if let Some(from_peer_info) = from_peer_info {
                                let _ = p2p.notify_gossip_transaction_validity(
                                    from_peer_info,
                                    GossipsubMessageAcceptance::Reject,
                                );
                            }
                            return;
                        }
                        Err(_) => {
                            if let Some(from_peer_info) = from_peer_info {
                                let _ = p2p.notify_gossip_transaction_validity(
                                    from_peer_info,
                                    GossipsubMessageAcceptance::Ignore,
                                );
                            }
                            return;
                        }
                    };
                    let tx = Arc::new(checked_tx);

                    let result = {
                        let mut pool = pool.write();
                        // TODO: Return the result of the insertion (see: https://github.com/FuelLabs/fuel-core/issues/2185)
                        pool.insert(tx.clone())
                    };

                    // P2P notification
                    match (from_peer_info, result.is_ok()) {
                        (Some(from_peer_info), true) => {
                            let _ = p2p.notify_gossip_transaction_validity(
                                from_peer_info,
                                GossipsubMessageAcceptance::Accept,
                            );
                        }
                        (Some(from_peer_info), false) => {
                            let _ = p2p.notify_gossip_transaction_validity(
                                from_peer_info,
                                GossipsubMessageAcceptance::Ignore,
                            );
                        }
                        (None, _) => {
                            if let Err(e) = p2p.broadcast_transaction(tx_clone) {
                                tracing::error!("Failed to broadcast transaction: {}", e);
                            }
                        }
                    };

                    if let Ok(removed) = &result {
                        new_txs_notifier.notify_waiters();
                        let submitted_time = SystemTime::now();
                        time_txs_submitted
                            .write()
                            .push_front((submitted_time, tx.id()));
                        for tx in removed {
                            tx_status_sender.send_squeezed_out(
                                tx.id(),
                                Error::Removed(RemovedReason::LessWorth(tx.id())),
                            );
                        }
                        tx_status_sender.send_submitted(
                            tx.id(),
                            Tai64::from_unix(
                                submitted_time
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .expect("Time can't be less than UNIX EPOCH")
                                    .as_secs() as i64,
                            ),
                        );
                    }
                }
            });
            if result.is_err() {
                if let Some(response_channel) = response_channel {
                    response_channel
                        .send(Err(Error::TooManyQueuedTransactions))
                        .map_err(|_| anyhow::anyhow!("Failed to send error"))?;
                }
                return Ok(());
            }
        }
        if let Some(response_channel) = response_channel {
            response_channel
                .send(Ok(()))
                .map_err(|_| anyhow::anyhow!("Failed to return result"))?;
        }
        Ok(())
    }

    fn manage_remove_coin_dependents(
        &self,
        tx_id: TxId,
        response_channel: oneshot::Sender<Vec<ArcPoolTx>>,
    ) -> anyhow::Result<()> {
        let dependents =
            self.pool
                .write()
                .remove_coin_dependents(tx_id)
                .map_err(|e| {
                    tracing::error!("Failed to remove coin dependents: {}", e);
                    anyhow::anyhow!(e)
                })?;
        response_channel
            .send(dependents)
            .map_err(|_| anyhow::anyhow!("Failed to send the result"))
    }

    async fn manage_tx_from_p2p(
        &mut self,
        tx: Transaction,
        message_id: Vec<u8>,
        peer_id: PeerId,
    ) -> anyhow::Result<()> {
        self.write_pool_requests_sender
            .send(WritePoolRequest::InsertTxs {
                transactions: vec![Arc::new(tx)],
                from_peer_info: Some(GossipsubMessageInfo {
                    message_id,
                    peer_id,
                }),
                response_channel: None,
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(())
    }

    fn manage_new_peer_subscribed(&mut self, peer_id: PeerId) {
        // We are not affected if there is too many queued job and we don't manage this peer.
        let _ = self.heavy_p2p_sync_processor.spawn({
            let p2p = self.p2p.clone();
            let pool = self.pool.clone();
            let txs_insert_sender = self.write_pool_requests_sender.clone();
            async move {
                let peer_tx_ids = p2p
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
                let tx_ids_to_ask: Vec<TxId> = {
                    let pool = pool.read();
                    peer_tx_ids
                        .into_iter()
                        .filter(|tx_id| !pool.contains(tx_id))
                        .collect()
                };
                if tx_ids_to_ask.is_empty() {
                    return;
                }
                let txs: Vec<Arc<Transaction>> = p2p
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
                let _ = txs_insert_sender
                    .send(WritePoolRequest::InsertTxs {
                        transactions: txs,
                        from_peer_info: None,
                        response_channel: None,
                    })
                    .await;
            }
        });
    }

    fn manage_prune_old_transactions(&mut self) {
        let txs_to_remove = {
            let mut time_txs_submitted = self.time_txs_submitted.write();
            let now = SystemTime::now();
            let mut txs_to_remove = vec![];
            while let Some((time, _)) = time_txs_submitted.back() {
                let Ok(duration) = now.duration_since(*time) else {
                    tracing::error!("Failed to calculate the duration since the transaction was submitted");
                    return;
                };
                if duration < self.txs_ttl {
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

    fn manage_find_transactions(&self, request: ReadPoolRequest) -> anyhow::Result<()> {
        match request {
            ReadPoolRequest::GetTxIds {
                max_txs,
                response_channel,
            } => {
                let tx_ids = {
                    let pool = self.pool.read();
                    pool.iter_tx_ids().take(max_txs).copied().collect()
                };
                response_channel
                    .send(tx_ids)
                    .map_err(|_| anyhow::anyhow!("Failed to send the result"))
            }
            ReadPoolRequest::GetTxs {
                tx_ids,
                response_channel,
            } => {
                let txs = {
                    let pool = self.pool.read();
                    tx_ids
                        .into_iter()
                        .map(|tx_id| {
                            pool.find_one(&tx_id).map(|stored_data| TxInfo {
                                tx: stored_data.transaction.clone(),
                                creation_instant: stored_data.creation_instant,
                            })
                        })
                        .collect()
                };
                response_channel
                    .send(txs)
                    .map_err(|_| anyhow::anyhow!("Failed to send the result"))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new_service<
    P2P,
    BlockImporter,
    PSProvider,
    PSView,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
>(
    config: Config,
    p2p: P2P,
    block_importer: BlockImporter,
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    wasm_checker: WasmChecker,
    memory_pool: MemoryPool,
) -> Service<
    P2P,
    PSProvider,
    ConsensusParamsProvider,
    GasPriceProvider,
    WasmChecker,
    MemoryPool,
>
where
    P2P: P2PTrait<GossipedTransaction = TransactionGossipData> + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView>,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    MemoryPool: MemoryPoolTrait + Send + Sync,
    BlockImporter: BlockImporterTrait + Send + Sync,
    P2P: P2PTrait + Send + Sync,
{
    let mut ttl_timer = tokio::time::interval(config.ttl_check_interval);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let tx_from_p2p_stream = p2p.gossiped_transaction_events();
    let new_peers_subscribed_stream = p2p.subscribe_new_peers();
    let new_txs_notifier = Arc::new(Notify::new());
    // TODO: Config the size
    let (write_pool_requests_sender, write_pool_requests_receiver) = mpsc::channel(10);
    let (select_transactions_requests_sender, select_transactions_requests_receiver) =
        mpsc::channel(10);
    let (read_pool_requests_sender, read_pool_requests_receiver) = mpsc::channel(10);
    let tx_status_sender = TxStatusChange::new(
        config.max_tx_update_subscriptions,
        // The connection should be closed automatically after the `SqueezedOut` event.
        // But because of slow/malicious consumers, the subscriber can still be occupied.
        // We allow the subscriber to receive the event produced by TxPool's TTL.
        // But we still want to drop subscribers after `2 * TxPool_TTL`.
        config.max_txs_ttl.saturating_mul(2),
    );
    let gas_price_provider = Arc::new(gas_price_provider);
    let consensus_parameters_provider = Arc::new(consensus_parameters_provider);
    Service::new(Task {
        new_peers_subscribed_stream,
        tx_from_p2p_stream,
        imported_block_results_stream: block_importer.block_events(),
        txs_ttl: config.max_txs_ttl,
        write_pool_requests_sender: write_pool_requests_sender.clone(),
        write_pool_requests_receiver,
        select_transactions_requests_receiver,
        read_pool_requests_receiver,
        consensus_parameters_provider: consensus_parameters_provider.clone(),
        gas_price_provider: gas_price_provider.clone(),
        wasm_checker: Arc::new(wasm_checker),
        memory: Arc::new(memory_pool),
        current_height: Arc::new(RwLock::new(current_height)),
        heavy_verif_insert_processor: Arc::new(
            HeavyAsyncProcessor::new(
                config.heavy_work.number_threads_to_verify_transactions,
                config.heavy_work.size_of_verification_queue,
            )
            .unwrap(),
        ),
        heavy_p2p_sync_processor: Arc::new(
            HeavyAsyncProcessor::new(
                config.heavy_work.number_threads_p2p_sync,
                config.heavy_work.size_of_p2p_sync_queue,
            )
            .unwrap(),
        ),
        pool: Arc::new(RwLock::new(Pool::new(
            ps_provider,
            GraphStorage::new(GraphConfig {
                max_txs_chain_count: config.max_txs_chain_count,
            }),
            BasicCollisionManager::new(),
            RatioTipGasSelection::new(gas_price_provider, consensus_parameters_provider),
            config,
        ))),
        p2p: Arc::new(p2p),
        new_txs_notifier: new_txs_notifier.clone(),
        time_txs_submitted: Arc::new(RwLock::new(VecDeque::new())),
        tx_status_sender: tx_status_sender.clone(),
        shared_state: SharedState {
            write_pool_requests_sender,
            tx_status_sender,
            select_transactions_requests_sender,
            read_pool_requests_sender,
            new_txs_notifier: new_txs_notifier.clone(),
        },
        ttl_timer,
    })
}
