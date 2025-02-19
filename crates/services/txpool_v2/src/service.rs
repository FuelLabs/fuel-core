use crate::{
    self as fuel_core_txpool,
    pool::TxPoolStats,
    pool_worker::{
        PoolInsertRequest,
        PoolNotification,
        PoolReadRequest,
        PoolWorkerInterface,
    },
};
use fuel_core_services::TaskNextAction;

use fuel_core_metrics::txpool_metrics::txpool_metrics;
use fuel_core_services::{
    seqlock::{
        SeqLock,
        SeqLockReader,
        SeqLockWriter,
    },
    AsyncProcessor,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    SyncProcessor,
};
use fuel_core_txpool::{
    collision_manager::basic::BasicCollisionManager,
    config::Config,
    error::{
        Error,
        RemovedReason,
    },
    pool::Pool,
    ports::{
        AtomicView,
        BlockImporter as BlockImporterTrait,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderTrait,
        P2PRequests,
        P2PSubscriptions,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
    },
    selection_algorithms::ratio_tip_gas::RatioTipGasSelection,
    service::{
        memory::MemoryPool,
        pruner::TransactionPruner,
        subscriptions::Subscriptions,
        verifications::Verification,
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
};
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
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
use std::{
    collections::{
        BTreeMap,
        HashSet,
        VecDeque,
    },
    sync::Arc,
    time::{
        SystemTime,
        SystemTimeError,
    },
};
use tokio::{
    sync::{
        mpsc,
        oneshot,
        watch,
    },
    time::MissedTickBehavior,
};

pub(crate) mod memory;
mod pruner;
mod subscriptions;
pub(crate) mod verifications;

pub type TxPool = Pool<
    GraphStorage,
    <GraphStorage as Storage>::StorageIndex,
    BasicCollisionManager<<GraphStorage as Storage>::StorageIndex>,
    RatioTipGasSelection<GraphStorage>,
>;

pub(crate) type Shared<T> = Arc<RwLock<T>>;

pub type Service<View, P2P> = ServiceRunner<Task<View, P2P>>;

/// Structure returned to others modules containing the transaction and
/// some useful infos
#[derive(Debug)]
pub struct TxInfo {
    /// The transaction
    pub tx: ArcPoolTx,
    /// The creation instant of the transaction
    pub creation_instant: SystemTime,
}

impl TxInfo {
    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn creation_instant(&self) -> &SystemTime {
        &self.creation_instant
    }
}

impl TryFrom<TxInfo> for TransactionStatus {
    type Error = SystemTimeError;

    fn try_from(value: TxInfo) -> Result<Self, Self::Error> {
        let unit_time = value
            .creation_instant
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs() as i64;

        Ok(TransactionStatus::Submitted {
            time: Tai64::from_unix(unit_time),
        })
    }
}

pub enum WritePoolRequest {
    InsertTxs {
        transactions: Vec<Arc<Transaction>>,
    },
    InsertTx {
        transaction: Arc<Transaction>,
        response_channel: oneshot::Sender<Result<(), Error>>,
    },
    RemoveCoinDependents {
        transactions: Vec<(TxId, String)>,
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

pub struct Task<View, P2P> {
    chain_id: ChainId,
    utxo_validation: bool,
    subscriptions: Subscriptions,
    verification: Verification<View>,
    p2p: Arc<P2P>,
    transaction_verifier_process: SyncProcessor,
    p2p_sync_process: AsyncProcessor,
    pruner: TransactionPruner,
    pool_worker: PoolWorkerInterface,
    current_height_writer: SeqLockWriter<BlockHeight>,
    current_height_reader: SeqLockReader<BlockHeight>,
    tx_sync_history: Shared<HashSet<PeerId>>,
    shared_state: SharedState,
    metrics: bool,
}

#[async_trait::async_trait]
impl<View, P2P> RunnableService for Task<View, P2P>
where
    View: TxPoolPersistentStorage,
    P2P: P2PRequests,
{
    const NAME: &'static str = "TxPool";

    type SharedData = SharedState;

    type Task = Task<View, P2P>;

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

impl<View, P2P> RunnableTask for Task<View, P2P>
where
    View: TxPoolPersistentStorage,
    P2P: P2PRequests,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                self.pool_worker.stop();
                TaskNextAction::Stop
            }

            block_result = self.subscriptions.imported_blocks.next() => {
                if let Some(result) = block_result {
                    self.import_block(result);
                    TaskNextAction::Continue
                } else {
                    self.pool_worker.stop();
                    TaskNextAction::Stop
                }
            }

            _ = self.pruner.ttl_timer.tick() => {
                self.try_prune_transactions();
                TaskNextAction::Continue
            }

            pool_notification = self.pool_worker.notification_receiver.recv() => {
                if let Some(notification) = pool_notification {
                    self.process_notification(notification);
                    TaskNextAction::Continue
                } else {
                    self.pool_worker.stop();
                    TaskNextAction::Stop
                }
            }

            write_pool_request = self.subscriptions.write_pool.recv() => {
                if let Some(write_pool_request) = write_pool_request {
                    self.process_write(write_pool_request);
                    TaskNextAction::Continue
                } else {
                    TaskNextAction::Continue
                }
            }

            tx_from_p2p = self.subscriptions.new_tx.next() => {
                if let Some(GossipData { data, message_id, peer_id }) = tx_from_p2p {
                    if let Some(tx) = data {
                        self.manage_tx_from_p2p(tx, message_id, peer_id);
                    }
                    TaskNextAction::Continue
                } else {
                    self.pool_worker.stop();
                    TaskNextAction::Stop
                }
            }

            new_peer_subscribed = self.subscriptions.new_tx_source.next() => {
                if let Some(peer_id) = new_peer_subscribed {
                    self.manage_new_peer_subscribed(peer_id);
                    TaskNextAction::Continue
                } else {
                    self.pool_worker.stop();
                    TaskNextAction::Stop
                }
            }

            read_pool_request = self.subscriptions.read_pool.recv() => {
                if let Some(read_pool_request) = read_pool_request {
                    self.process_read(read_pool_request);
                    TaskNextAction::Continue
                } else {
                    self.pool_worker.stop();
                    TaskNextAction::Stop
                }
            }
        }
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        self.pool_worker.stop();
        Ok(())
    }
}

impl<View, P2P> Task<View, P2P>
where
    View: TxPoolPersistentStorage,
    P2P: P2PRequests,
{
    fn import_block(&mut self, result: SharedImportResult) {
        dbg!("import_block");
        let new_height = *result.sealed_block.entity.header().height();
        let executed_transaction = result.tx_status.iter().map(|s| s.id).collect();
        // We don't want block importer wait for us to process the result.
        drop(result);

        self.pool_worker.remove(executed_transaction);

        {
            self.current_height_writer.write(|data| {
                *data = new_height;
            });
        }

        // Remove expired transactions
        let range_to_remove = self
            .pruner
            .height_expiration_txs
            .range(..=new_height)
            .map(|(k, _)| *k)
            .collect::<Vec<_>>();
        for height in range_to_remove {
            let expired_txs = self.pruner.height_expiration_txs.remove(&height);
            if let Some(expired_txs) = expired_txs {
                self.pool_worker.remove_and_coin_dependents((
                    expired_txs,
                    Error::Removed(RemovedReason::Ttl),
                ));
            }
        }
    }

    fn process_write(&self, write_pool_request: WritePoolRequest) {
        match write_pool_request {
            WritePoolRequest::InsertTxs { transactions } => {
                self.insert_transactions(transactions);
            }
            WritePoolRequest::InsertTx {
                transaction,
                response_channel,
            } => {
                if let Ok(reservation) = self.transaction_verifier_process.reserve() {
                    let op = self.insert_transaction(
                        transaction,
                        None,
                        Some(response_channel),
                    );

                    self.transaction_verifier_process
                        .spawn_reserved(reservation, op);
                } else {
                    tracing::error!("Failed to insert transaction: Out of capacity");
                    let _ = response_channel.send(Err(Error::ServiceQueueFull));
                }
            }
            WritePoolRequest::RemoveCoinDependents { transactions } => {
                self.manage_remove_coin_dependents(transactions);
            }
        }
    }

    fn process_notification(&mut self, notification: PoolNotification) {
        match notification {
            PoolNotification::Inserted {
                tx_id,
                time,
                expiration,
                from_peer_info,
                tx,
            } => {
                if let Some(from_peer_info) = from_peer_info {
                    let _ = self.p2p.notify_gossip_transaction_validity(
                        from_peer_info,
                        GossipsubMessageAcceptance::Accept,
                    );
                } else if let Err(e) = self.p2p.broadcast_transaction(tx) {
                    tracing::error!("Failed to broadcast transaction: {}", e);
                }
                self.pruner.time_txs_submitted.push_front((time, tx_id));

                let duration = time
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Time can't be less than UNIX EPOCH");

                self.shared_state
                    .tx_status_sender
                    .send_submitted(tx_id, Tai64::from_unix(duration.as_secs() as i64));

                if expiration < u32::MAX.into() {
                    let block_height_expiration = self
                        .pruner
                        .height_expiration_txs
                        .entry(expiration)
                        .or_default();
                    block_height_expiration.push(tx_id);
                }
                self.shared_state.new_txs_notifier.send_replace(());
            }
            PoolNotification::ErrorInsertion { tx_id, error, from_peer_info } => {
                if let Some(from_peer_info) = from_peer_info {
                    let _ = self.p2p.notify_gossip_transaction_validity(
                        from_peer_info,
                        GossipsubMessageAcceptance::Reject,
                    );
                }
                self.shared_state.tx_status_sender.send_squeezed_out(tx_id, error);
            }
            PoolNotification::Removed { tx_id, error } => {
                self.shared_state
                    .tx_status_sender
                    .send_squeezed_out(tx_id, error);
            }
        }
    }

    fn insert_transactions(&self, transactions: Vec<Arc<Transaction>>) {
        for transaction in transactions {
            let Ok(reservation) = self.transaction_verifier_process.reserve() else {
                tracing::error!("Failed to insert transactions: Out of capacity");
                continue
            };
            let op = self.insert_transaction(transaction, None, None);

            self.transaction_verifier_process
                .spawn_reserved(reservation, op);
        }
    }

    fn insert_transaction(
        &self,
        transaction: Arc<Transaction>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    ) -> impl FnOnce() + Send + 'static {
        let metrics = self.metrics;
        if metrics {
            txpool_metrics()
                .number_of_transactions_pending_verification
                .inc();
        }

        let p2p = self.p2p.clone();
        let verification = self.verification.clone();
        let pool_insert_request_sender = self.pool_worker.request_insert_sender.clone();
        let shared_state = self.shared_state.clone();
        let current_height_reader = self.current_height_reader.clone();
        let tx_id = transaction.id(&self.chain_id);
        let utxo_validation = self.utxo_validation;

        let insert_transaction_thread_pool_op = move || {
            let current_height = current_height_reader.read();

            let tx_clone = Arc::clone(&transaction);
            // TODO: This should be removed if the checked transactions
            //  can work with Arc in it
            //  (see https://github.com/FuelLabs/fuel-vm/issues/831)
            let transaction = Arc::unwrap_or_clone(transaction);

            let result = verification.perform_all_verifications(
                transaction,
                current_height,
                utxo_validation,
            );

            if metrics {
                txpool_metrics()
                    .number_of_transactions_pending_verification
                    .dec();
            }

            let checked_tx = match result {
                Ok(checked_tx) => checked_tx,
                Err(err) => {
                    if let Some(channel) = response_channel {
                        let _ = channel.send(Err(err.clone()));
                    }

                    if let Some(from_peer_info) = from_peer_info {
                        match err {
                            fuel_core_txpool::error::Error::ConsensusValidity(_)
                            | fuel_core_txpool::error::Error::MintIsDisallowed => {
                                let _ = p2p.notify_gossip_transaction_validity(
                                    from_peer_info,
                                    GossipsubMessageAcceptance::Reject,
                                );
                            }
                            _ => {
                                let _ = p2p.notify_gossip_transaction_validity(
                                    from_peer_info,
                                    GossipsubMessageAcceptance::Ignore,
                                );
                            }
                        }
                    }
                    shared_state.tx_status_sender.send_squeezed_out(tx_id, err);
                    return
                }
            };

            let tx = Arc::new(checked_tx);
            // We don't use the `insert` from `PoolWorkerInterface` because we don't want to make the whole object cloneable
            if let Err(e) = pool_insert_request_sender.send(PoolInsertRequest::Insert {
                tx,
                tx_for_p2p: tx_clone,
                from_peer_info,
                response_channel,
            }) {
                tracing::error!("Failed to send the insert request: {}", e);
            }
        };
        move || {
            if metrics {
                let start_time = tokio::time::Instant::now();
                insert_transaction_thread_pool_op();
                let time_for_task_to_complete = start_time.elapsed().as_micros();
                txpool_metrics()
                    .transaction_insertion_time_in_thread_pool_microseconds
                    .observe(time_for_task_to_complete as f64);
            } else {
                insert_transaction_thread_pool_op();
            }
        }
    }

    fn manage_remove_coin_dependents(&self, transactions: Vec<(TxId, String)>) {
        self.pool_worker.remove_coin_dependents(transactions);
    }

    fn manage_tx_from_p2p(
        &mut self,
        tx: Transaction,
        message_id: Vec<u8>,
        peer_id: PeerId,
    ) {
        let Ok(reservation) = self.transaction_verifier_process.reserve() else {
            tracing::error!("Failed to insert transaction from P2P: Out of capacity");
            return;
        };

        let info = Some(GossipsubMessageInfo {
            message_id,
            peer_id,
        });
        let op = self.insert_transaction(Arc::new(tx), info, None);
        self.transaction_verifier_process
            .spawn_reserved(reservation, op);
    }

    fn manage_new_peer_subscribed(&mut self, peer_id: PeerId) {
        // We are not affected if there is too many queued job and we don't manage this peer.
        let _ = self.p2p_sync_process.try_spawn({
            let p2p = self.p2p.clone();
            let request_sender = self.pool_worker.request_read_sender.clone();
            let txs_insert_sender = self.shared_state.write_pool_requests_sender.clone();
            let tx_sync_history = self.tx_sync_history.clone();
            async move {
                {
                    let mut tx_sync_history = tx_sync_history.write();

                    // We already synced with this peer in the past.
                    if !tx_sync_history.insert(peer_id.clone()) {
                        return
                    }
                }

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

                // We don't use the `get_non_existing_txs` from `PoolWorkerInterface` because we don't want to make the whole object cloneable
                let (response_sender, response_receiver) = oneshot::channel();
                if let Err(e) = request_sender.send(PoolReadRequest::NonExistingTxs {
                    tx_ids: peer_tx_ids,
                    non_existing_txs: response_sender,
                }) {
                    tracing::error!(
                        "Failed to send the request to get non existing txs: {}",
                        e
                    );
                    return;
                }

                let tx_ids_to_ask = match response_receiver.await {
                    Ok(tx_ids) => tx_ids,
                    Err(e) => {
                        tracing::error!("Failed to receive the non existing txs: {}", e);
                        return;
                    }
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
                    .try_send(WritePoolRequest::InsertTxs { transactions: txs });
            }
        });
    }

    fn try_prune_transactions(&mut self) {
        dbg!("try_prune_transactions");
        let mut txs_to_remove = vec![];
        {
            let now = SystemTime::now();
            while let Some((time, _)) = self.pruner.time_txs_submitted.back() {
                let Ok(duration) = now.duration_since(*time) else {
                    tracing::error!("Failed to calculate the duration since the transaction was submitted");
                    return;
                };
                if duration < self.pruner.txs_ttl {
                    break;
                }
                // SAFETY: We are removing the last element that we just checked
                txs_to_remove
                    .push(self.pruner.time_txs_submitted.pop_back().expect("qed").1);
            }
        }

        self.pool_worker.remove_and_coin_dependents((
            txs_to_remove,
            Error::Removed(RemovedReason::Ttl),
        ));

        {
            // Each time when we prune transactions, clear the history of synchronization
            // to have a chance to sync this transaction again from other peers.
            let mut tx_sync_history = self.tx_sync_history.write();
            tx_sync_history.clear();
        }
    }

    fn process_read(&self, request: ReadPoolRequest) {
        match request {
            ReadPoolRequest::GetTxIds {
                max_txs,
                response_channel,
            } => {
                self.pool_worker.get_tx_ids(max_txs, response_channel);
            }
            ReadPoolRequest::GetTxs {
                tx_ids,
                response_channel,
            } => {
                self.pool_worker.get_txs(tx_ids, response_channel);
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
>(
    chain_id: ChainId,
    config: Config,
    p2p: P2P,
    block_importer: BlockImporter,
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    wasm_checker: WasmChecker,
) -> Service<PSView, P2P>
where
    P2P: P2PSubscriptions<GossipedTransaction = TransactionGossipData>,
    P2P: P2PRequests,
    PSProvider: AtomicView<LatestView = PSView> + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider,
    GasPriceProvider: GasPriceProviderTrait,
    WasmChecker: WasmCheckerTrait,
    BlockImporter: BlockImporterTrait,
{
    let mut ttl_timer = tokio::time::interval(config.ttl_check_interval);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let tx_from_p2p_stream = p2p.gossiped_transaction_events();
    let new_peers_subscribed_stream = p2p.subscribe_new_peers();

    let (write_pool_requests_sender, write_pool_requests_receiver) = mpsc::channel(
        config
            .service_channel_limits
            .max_pending_write_pool_requests,
    );
    let (read_pool_requests_sender, read_pool_requests_receiver) =
        mpsc::channel(config.service_channel_limits.max_pending_read_pool_requests);
    let (pool_stats_sender, pool_stats_receiver) =
        tokio::sync::watch::channel(TxPoolStats::default());
    let tx_status_sender = TxStatusChange::new(
        config.max_tx_update_subscriptions,
        // The connection should be closed automatically after the `SqueezedOut` event.
        // But because of slow/malicious consumers, the subscriber can still be occupied.
        // We allow the subscriber to receive the event produced by TxPool's TTL.
        // But we still want to drop subscribers after `2 * TxPool_TTL`.
        config.max_txs_ttl.saturating_mul(2),
    );
    let (new_txs_notifier, _) = watch::channel(());

    let subscriptions = Subscriptions {
        new_tx_source: new_peers_subscribed_stream,
        new_tx: tx_from_p2p_stream,
        imported_blocks: block_importer.block_events(),
        write_pool: write_pool_requests_receiver,
        read_pool: read_pool_requests_receiver,
    };

    let storage_provider = Arc::new(ps_provider);
    let verification = Verification {
        persistent_storage_provider: storage_provider.clone(),
        consensus_parameters_provider: Arc::new(consensus_parameters_provider),
        gas_price_provider: Arc::new(gas_price_provider),
        wasm_checker: Arc::new(wasm_checker),
        memory_pool: MemoryPool::new(),
        blacklist: config.black_list.clone(),
    };

    let pruner = TransactionPruner {
        txs_ttl: config.max_txs_ttl,
        time_txs_submitted: VecDeque::new(),
        height_expiration_txs: BTreeMap::new(),
        ttl_timer,
    };

    let transaction_verifier_process = SyncProcessor::new(
        "TxPool_TxVerifierProcessor",
        config.heavy_work.number_threads_to_verify_transactions,
        config.heavy_work.size_of_verification_queue,
    )
    .unwrap();

    let p2p_sync_process = AsyncProcessor::new(
        "TxPool_P2PSynchronizationProcessor",
        config.heavy_work.number_threads_p2p_sync,
        config.heavy_work.size_of_p2p_sync_queue,
    )
    .unwrap();

    let metrics = config.metrics;

    let utxo_validation = config.utxo_validation;
    let txpool = Pool::new(
        GraphStorage::new(GraphConfig {
            max_txs_chain_count: config.max_txs_chain_count,
        }),
        BasicCollisionManager::new(),
        RatioTipGasSelection::new(),
        config,
        pool_stats_sender,
    );

    // BlockHeight is < 64 bytes, so we can use SeqLock
    let (current_height_writer, current_height_reader) =
        unsafe { SeqLock::new(current_height) };

    let pool_worker = PoolWorkerInterface::new(txpool, storage_provider);

    let shared_state = SharedState {
        write_pool_requests_sender,
        tx_status_sender,
        select_transactions_requests_sender: pool_worker
            .extract_block_transactions_sender
            .clone(),
        read_pool_requests_sender,
        new_txs_notifier,
        latest_stats: pool_stats_receiver,
    };

    Service::new(Task {
        chain_id,
        utxo_validation,
        subscriptions,
        verification,
        transaction_verifier_process,
        p2p_sync_process,
        pruner,
        p2p: Arc::new(p2p),
        current_height_writer,
        current_height_reader,
        pool_worker,
        shared_state,
        metrics,
        tx_sync_history: Default::default(),
    })
}
