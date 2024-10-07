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
        P2PRequests,
        P2PSubscriptions,
        TxPoolPersistentStorage,
        WasmChecker as WasmCheckerTrait,
    },
    selection_algorithms::{
        ratio_tip_gas::RatioTipGasSelection,
        Constraints,
    },
    service::{
        memory::MemoryPool,
        p2p::P2PExt,
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
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
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
    collections::VecDeque,
    future::Future,
    sync::Arc,
    time::SystemTime,
};
use tokio::{
    sync::{
        mpsc,
        oneshot,
        Notify,
    },
    time::MissedTickBehavior,
};

pub(crate) mod memory;
mod p2p;
mod pruner;
mod subscriptions;
pub(crate) mod verifications;

pub type TxPool = Pool<
    GraphStorage,
    <GraphStorage as Storage>::StorageIndex,
    BasicCollisionManager<<GraphStorage as Storage>::StorageIndex>,
    RatioTipGasSelection<GraphStorage>,
>;

pub type Shared<T> = Arc<RwLock<T>>;

pub type Service<View> = ServiceRunner<Task<View>>;

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
        TransactionStatus::Submitted {
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

pub struct SelectTransactionsRequest {
    pub constraints: Constraints,
    pub response_channel: oneshot::Sender<Vec<ArcPoolTx>>,
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

pub struct Task<View> {
    chain_id: ChainId,
    subscriptions: Subscriptions,
    verification: Verification<View>,
    p2p: Arc<dyn P2PRequests>,
    transaction_verifier_process: HeavyAsyncProcessor,
    p2p_sync_process: HeavyAsyncProcessor,
    pruner: TransactionPruner,
    pool: Shared<TxPool>,
    current_height: Arc<RwLock<BlockHeight>>,
    shared_state: SharedState,
}

#[async_trait::async_trait]
impl<View> RunnableService for Task<View>
where
    View: TxPoolPersistentStorage,
{
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState;

    type Task = Task<View>;

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
impl<View> RunnableTask for Task<View>
where
    View: TxPoolPersistentStorage,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            block_result = self.subscriptions.imported_blocks.next() => {
                if let Some(result) = block_result {
                    self.import_block(result);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            select_transaction_request = self.subscriptions.extract_transactions.recv() => {
                if let Some(select_transaction_request) = select_transaction_request {
                    self.extract_transaction(select_transaction_request);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            _ = self.pruner.ttl_timer.tick() => {
                self.try_prune_transactions();
                should_continue = true;
            }

            write_pool_request = self.subscriptions.write_pool.recv() => {
                if let Some(write_pool_request) = write_pool_request {
                    self.process_write(write_pool_request);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            tx_from_p2p = self.subscriptions.new_tx.next() => {
                if let Some(GossipData { data, message_id, peer_id }) = tx_from_p2p {
                    if let Some(tx) = data {
                        self.manage_tx_from_p2p(tx, message_id, peer_id);
                    }
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_peer_subscribed = self.subscriptions.new_tx_source.next() => {
                if let Some(peer_id) = new_peer_subscribed {
                    self.manage_new_peer_subscribed(peer_id);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            read_pool_request = self.subscriptions.read_pool.recv() => {
                if let Some(read_pool_request) = read_pool_request {
                    self.process_read(read_pool_request);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }
        }

        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<View> Task<View>
where
    View: TxPoolPersistentStorage,
{
    fn import_block(&mut self, result: SharedImportResult) {
        let new_height = *result.sealed_block.entity.header().height();
        let executed_transaction = result.tx_status.iter().map(|s| s.id).collect();
        // We don't want block importer way for us to process the result.
        drop(result);

        {
            let mut tx_pool = self.pool.write();
            tx_pool.remove_transaction(executed_transaction);
        }

        {
            let mut block_height = self.current_height.write();
            *block_height = new_height;
        }
    }

    fn extract_transaction(&self, request: SelectTransactionsRequest) {
        let SelectTransactionsRequest {
            constraints,
            response_channel,
        } = request;

        let result = self
            .pool
            .write()
            .extract_transactions_for_block(constraints);

        if let Err(e) = response_channel.send(result) {
            tracing::error!("Failed to send the result: {:?}", e);
            // TODO: We need to remove all dependencies of the transactions that we failed to send
        }
    }

    fn process_write(&self, write_pool_request: WritePoolRequest) {
        match write_pool_request {
            WritePoolRequest::InsertTxs { transactions } => {
                self.insert_transactions(transactions);
            }
            WritePoolRequest::RemoveCoinDependents { transactions } => {
                self.manage_remove_coin_dependents(transactions);
            }
            WritePoolRequest::InsertTx {
                transaction,
                response_channel,
            } => {
                if let Ok(reservation) = self.transaction_verifier_process.reserve() {
                    let future = self.insert_transaction(
                        transaction,
                        None,
                        Some(response_channel),
                    );

                    self.transaction_verifier_process
                        .spawn_reserved(reservation, future);
                } else {
                    tracing::error!("Failed to insert transaction: Out of capacity");
                    let _ = response_channel.send(Err(Error::ServiceQueueFull));
                }
            }
        }
    }

    fn insert_transactions(&self, transactions: Vec<Arc<Transaction>>) {
        for transaction in transactions {
            let Ok(reservation) = self.transaction_verifier_process.reserve() else {
                tracing::error!("Failed to insert transactions: Out of capacity");
                continue
            };
            let future = self.insert_transaction(transaction, None, None);

            self.transaction_verifier_process
                .spawn_reserved(reservation, future);
        }
    }

    fn insert_transaction(
        &self,
        transaction: Arc<Transaction>,
        from_peer_info: Option<GossipsubMessageInfo>,
        response_channel: Option<oneshot::Sender<Result<(), Error>>>,
    ) -> impl Future<Output = ()> + Send + 'static {
        let verification = self.verification.clone();
        let pool = self.pool.clone();
        let p2p = self.p2p.clone();
        let shared_state = self.shared_state.clone();
        let current_height = self.current_height.clone();
        let time_txs_submitted = self.pruner.time_txs_submitted.clone();
        let tx_id = transaction.id(&self.chain_id);

        async move {
            let current_height = *current_height.read();

            // TODO: This should be removed if the checked transactions
            //  can work with Arc in it
            //  (see https://github.com/FuelLabs/fuel-vm/issues/831)
            let transaction = Arc::unwrap_or_clone(transaction);

            let result = verification
                .perform_all_verifications(transaction, &pool, current_height)
                .await;

            p2p.process_insertion_result(from_peer_info, &result);

            let checked_tx = match result {
                Ok(checked_tx) => checked_tx,
                Err(err) => {
                    if let Some(channel) = response_channel {
                        let _ = channel.send(Err(err.clone()));
                    }

                    shared_state.tx_status_sender.send_squeezed_out(tx_id, err);
                    return
                }
            };

            let tx = Arc::new(checked_tx);

            let result = {
                let mut pool = pool.write();
                let result = verification.persistent_storage_provider.latest_view();

                match result {
                    Ok(view) => pool.insert(tx, &view),
                    Err(err) => Err(Error::Database(format!("{:?}", err))),
                }
            };

            let removed_txs = match result {
                Ok(removed_txs) => {
                    let submitted_time = SystemTime::now();
                    shared_state.new_txs_notifier.notify_waiters();
                    time_txs_submitted
                        .write()
                        .push_front((submitted_time, tx_id));

                    let duration = submitted_time
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .expect("Time can't be less than UNIX EPOCH");

                    shared_state.tx_status_sender.send_submitted(
                        tx_id,
                        Tai64::from_unix(duration.as_secs() as i64),
                    );

                    if let Some(channel) = response_channel {
                        let _ = channel.send(Ok(()));
                    }

                    removed_txs
                }
                Err(err) => {
                    if let Some(channel) = response_channel {
                        let _ = channel.send(Err(err.clone()));
                    }

                    shared_state.tx_status_sender.send_squeezed_out(tx_id, err);
                    return
                }
            };

            for tx in removed_txs {
                shared_state.tx_status_sender.send_squeezed_out(
                    tx.id(),
                    Error::Removed(RemovedReason::LessWorth(tx.id())),
                );
            }
        }
    }

    fn manage_remove_coin_dependents(&self, transactions: Vec<(TxId, String)>) {
        for (tx_id, reason) in transactions {
            let dependents = self.pool.write().remove_coin_dependents(tx_id);

            for removed_tx in dependents {
                self.shared_state.tx_status_sender.send_squeezed_out(
                    removed_tx.id(),
                    Error::SkippedTransaction(
                        format!("Parent transaction with {tx_id}, was removed because of the {reason}")
                    )
                );
            }
        }
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
        let future = self.insert_transaction(Arc::new(tx), info, None);
        self.transaction_verifier_process
            .spawn_reserved(reservation, future);
    }

    fn manage_new_peer_subscribed(&mut self, peer_id: PeerId) {
        // We are not affected if there is too many queued job and we don't manage this peer.
        let _ = self.p2p_sync_process.spawn({
            let p2p = self.p2p.clone();
            let pool = self.pool.clone();
            let txs_insert_sender = self.shared_state.write_pool_requests_sender.clone();
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
                    .try_send(WritePoolRequest::InsertTxs { transactions: txs });
            }
        });
    }

    fn try_prune_transactions(&mut self) {
        let mut time_txs_submitted = self.pruner.time_txs_submitted.write();
        let now = SystemTime::now();
        let mut txs_to_remove = vec![];
        while let Some((time, _)) = time_txs_submitted.back() {
            let Ok(duration) = now.duration_since(*time) else {
                tracing::error!("Failed to calculate the duration since the transaction was submitted");
                return;
            };
            if duration < self.pruner.txs_ttl {
                break;
            }
            // SAFETY: We are removing the last element that we just checked
            txs_to_remove.push(time_txs_submitted.pop_back().expect("qed").1);
        }
        drop(time_txs_submitted);

        let mut pool = self.pool.write();
        let removed = pool.remove_transaction_and_dependents(txs_to_remove);
        drop(pool);

        for tx in removed {
            self.shared_state
                .tx_status_sender
                .send_squeezed_out(tx.id(), Error::Removed(RemovedReason::Ttl));
        }
    }

    fn process_read(&self, request: ReadPoolRequest) {
        match request {
            ReadPoolRequest::GetTxIds {
                max_txs,
                response_channel,
            } => {
                let tx_ids = {
                    let pool = self.pool.read();
                    pool.iter_tx_ids().take(max_txs).copied().collect()
                };
                if response_channel.send(tx_ids).is_err() {
                    tracing::error!(
                        "Failed to send the result back for `GetTxIds` request"
                    );
                }
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
                if response_channel.send(txs).is_err() {
                    tracing::error!(
                        "Failed to send the result back for `GetTxs` request"
                    );
                }
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
    config: Config,
    p2p: P2P,
    block_importer: BlockImporter,
    ps_provider: PSProvider,
    consensus_parameters_provider: ConsensusParamsProvider,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    wasm_checker: WasmChecker,
) -> Service<PSView>
where
    P2P: P2PSubscriptions<GossipedTransaction = TransactionGossipData>,
    P2P: P2PRequests + Send + Sync + 'static,
    PSProvider: AtomicView<LatestView = PSView> + Send + Sync + 'static,
    PSView: TxPoolPersistentStorage,
    ConsensusParamsProvider: ConsensusParametersProvider + Send + Sync,
    GasPriceProvider: GasPriceProviderTrait + Send + Sync,
    WasmChecker: WasmCheckerTrait + Send + Sync,
    BlockImporter: BlockImporterTrait + Send + Sync,
{
    let chain_id = consensus_parameters_provider
        .latest_consensus_parameters()
        .1
        .chain_id();

    let mut ttl_timer = tokio::time::interval(config.ttl_check_interval);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let tx_from_p2p_stream = p2p.gossiped_transaction_events();
    let new_peers_subscribed_stream = p2p.subscribe_new_peers();

    // TODO: Config the size
    let (write_pool_requests_sender, write_pool_requests_receiver) = mpsc::channel(
        config
            .service_channel_limits
            .max_pending_write_pool_requests,
    );
    let (select_transactions_requests_sender, select_transactions_requests_receiver) =
        mpsc::channel(
            config
                .service_channel_limits
                .max_pending_select_transactions_requests,
        );
    let (read_pool_requests_sender, read_pool_requests_receiver) =
        mpsc::channel(config.service_channel_limits.max_pending_read_pool_requests);
    let tx_status_sender = TxStatusChange::new(
        config.max_tx_update_subscriptions,
        // The connection should be closed automatically after the `SqueezedOut` event.
        // But because of slow/malicious consumers, the subscriber can still be occupied.
        // We allow the subscriber to receive the event produced by TxPool's TTL.
        // But we still want to drop subscribers after `2 * TxPool_TTL`.
        config.max_txs_ttl.saturating_mul(2),
    );
    let new_txs_notifier = Arc::new(Notify::new());

    let shared_state = SharedState {
        write_pool_requests_sender,
        tx_status_sender,
        select_transactions_requests_sender,
        read_pool_requests_sender,
        new_txs_notifier,
    };

    let subscriptions = Subscriptions {
        new_tx_source: new_peers_subscribed_stream,
        new_tx: tx_from_p2p_stream,
        imported_blocks: block_importer.block_events(),
        write_pool: write_pool_requests_receiver,
        extract_transactions: select_transactions_requests_receiver,
        read_pool: read_pool_requests_receiver,
    };

    let verification = Verification {
        persistent_storage_provider: Arc::new(ps_provider),
        consensus_parameters_provider: Arc::new(consensus_parameters_provider),
        gas_price_provider: Arc::new(gas_price_provider),
        wasm_checker: Arc::new(wasm_checker),
        memory_pool: MemoryPool::new(),
    };

    let pruner = TransactionPruner {
        txs_ttl: config.max_txs_ttl,
        time_txs_submitted: Arc::new(RwLock::new(VecDeque::new())),
        ttl_timer,
    };

    let transaction_verifier_process = HeavyAsyncProcessor::new(
        config.heavy_work.number_threads_to_verify_transactions,
        config.heavy_work.size_of_verification_queue,
    )
    .unwrap();

    let p2p_sync_process = HeavyAsyncProcessor::new(
        config.heavy_work.number_threads_p2p_sync,
        config.heavy_work.size_of_p2p_sync_queue,
    )
    .unwrap();

    let txpool = Pool::new(
        GraphStorage::new(GraphConfig {
            max_txs_chain_count: config.max_txs_chain_count,
        }),
        BasicCollisionManager::new(),
        RatioTipGasSelection::new(),
        config,
    );

    Service::new(Task {
        chain_id,
        subscriptions,
        verification,
        transaction_verifier_process,
        p2p_sync_process,
        pruner,
        p2p: Arc::new(p2p),
        current_height: Arc::new(RwLock::new(current_height)),
        pool: Arc::new(RwLock::new(txpool)),
        shared_state,
    })
}
