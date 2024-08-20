use std::{
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use parking_lot::Mutex as ParkingMutex;
use tokio::{
    sync::broadcast,
    time::MissedTickBehavior,
};
use tokio_stream::StreamExt;

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_importer::SharedImportResult,
        p2p::{
            GossipData,
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            TransactionGossipData,
        },
        txpool::{
            ArcPoolTx,
            InsertionResult,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use update_sender::UpdateSender;

use crate::{
    ports::{
        BlockImporter,
        ConsensusParametersProvider,
        GasPriceProvider as GasPriceProviderConstraint,
        MemoryPool,
        PeerToPeer,
        TxPoolDb,
        WasmChecker as WasmCheckerConstraint,
    },
    transaction_selector::select_transactions,
    txpool::{
        check_single_tx,
        check_transactions,
    },
    Config,
    Error as TxPoolError,
    TxInfo,
    TxPool,
};

use self::update_sender::{
    MpscChannel,
    TxStatusStream,
};

mod update_sender;

pub type Service<P2P, DB, WC, GP, CP, MP> = ServiceRunner<Task<P2P, DB, WC, GP, CP, MP>>;

#[derive(Clone)]
pub struct TxStatusChange {
    new_tx_notification_sender: broadcast::Sender<TxId>,
    update_sender: UpdateSender,
}

impl TxStatusChange {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        let (new_tx_notification_sender, _) = broadcast::channel(capacity);
        let update_sender = UpdateSender::new(capacity, ttl);
        Self {
            new_tx_notification_sender,
            update_sender,
        }
    }

    pub fn send_complete(
        &self,
        id: Bytes32,
        block_height: &BlockHeight,
        message: TxStatusMessage,
    ) {
        tracing::info!("Transaction {id} successfully included in block {block_height}");
        self.update_sender.send(TxUpdate::new(id, message));
    }

    pub fn send_submitted(&self, id: Bytes32, time: Tai64) {
        tracing::info!("Transaction {id} successfully submitted to the tx pool");
        let _ = self.new_tx_notification_sender.send(id);
        self.update_sender.send(TxUpdate::new(
            id,
            TxStatusMessage::Status(TransactionStatus::Submitted { time }),
        ));
    }

    pub fn send_squeezed_out(&self, id: Bytes32, reason: TxPoolError) {
        tracing::info!("Transaction {id} squeezed out because {reason}");
        self.update_sender.send(TxUpdate::new(
            id,
            TxStatusMessage::Status(TransactionStatus::SqueezedOut {
                reason: reason.to_string(),
            }),
        ));
    }
}

pub struct SharedState<
    P2P,
    ViewProvider,
    WasmChecker,
    GasPriceProvider,
    ConsensusProvider,
    MP,
> {
    tx_status_sender: TxStatusChange,
    txpool: Arc<ParkingMutex<TxPool<ViewProvider, WasmChecker>>>,
    p2p: Arc<P2P>,
    utxo_validation: bool,
    current_height: Arc<ParkingMutex<BlockHeight>>,
    consensus_parameters_provider: Arc<ConsensusProvider>,
    gas_price_provider: Arc<GasPriceProvider>,
    memory_pool: Arc<MP>,
}

impl<P2P, ViewProvider, GasPriceProvider, WasmChecker, ConsensusProvider, MP> Clone
    for SharedState<
        P2P,
        ViewProvider,
        WasmChecker,
        GasPriceProvider,
        ConsensusProvider,
        MP,
    >
{
    fn clone(&self) -> Self {
        Self {
            tx_status_sender: self.tx_status_sender.clone(),
            txpool: self.txpool.clone(),
            p2p: self.p2p.clone(),
            utxo_validation: self.utxo_validation,
            current_height: self.current_height.clone(),
            consensus_parameters_provider: self.consensus_parameters_provider.clone(),
            gas_price_provider: self.gas_price_provider.clone(),
            memory_pool: self.memory_pool.clone(),
        }
    }
}

pub struct Task<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP> {
    gossiped_tx_stream: BoxStream<TransactionGossipData>,
    committed_block_stream: BoxStream<SharedImportResult>,
    tx_pool_shared_state: SharedState<
        P2P,
        ViewProvider,
        WasmChecker,
        GasPriceProvider,
        ConsensusProvider,
        MP,
    >,
    ttl_timer: tokio::time::Interval,
}

#[async_trait::async_trait]
impl<P2P, ViewProvider, View, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
    RunnableService
    for Task<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<LatestView = View>,
    View: TxPoolDb,
    WasmChecker: WasmCheckerConstraint + Send + Sync,
    GasPriceProvider: GasPriceProviderConstraint + Send + Sync,
    ConsensusProvider: ConsensusParametersProvider + Send + Sync,
    MP: MemoryPool + Send + Sync,
{
    const NAME: &'static str = "TxPool";

    type SharedData = SharedState<
        P2P,
        ViewProvider,
        WasmChecker,
        GasPriceProvider,
        ConsensusProvider,
        MP,
    >;
    type Task =
        Task<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.tx_pool_shared_state.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        self.ttl_timer.reset();
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<P2P, ViewProvider, View, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
    RunnableTask
    for Task<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<LatestView = View>,
    View: TxPoolDb,
    WasmChecker: WasmCheckerConstraint + Send + Sync,
    GasPriceProvider: GasPriceProviderConstraint + Send + Sync,
    ConsensusProvider: ConsensusParametersProvider + Send + Sync,
    MP: MemoryPool + Send + Sync,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;

        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            _ = self.ttl_timer.tick() => {
                let removed = self.tx_pool_shared_state.txpool.lock().prune_old_txs();
                for tx in removed {
                    self.tx_pool_shared_state.tx_status_sender.send_squeezed_out(tx.id(), TxPoolError::TTLReason);
                }

                should_continue = true
            }

            result = self.committed_block_stream.next() => {
                if let Some(result) = result {
                    let new_height = *result
                        .sealed_block
                        .entity.header().height();

                    {
                        let mut lock = self.tx_pool_shared_state.txpool.lock();
                        lock.block_update(
                            &result.tx_status,
                        );
                        *self.tx_pool_shared_state.current_height.lock() = new_height;
                    }
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_transaction = self.gossiped_tx_stream.next() => {
                if let Some(GossipData { data: Some(tx), message_id, peer_id }) = new_transaction {
                    let current_height = *self.tx_pool_shared_state.current_height.lock();
                    let (version, params) = self
                        .tx_pool_shared_state
                        .consensus_parameters_provider
                        .latest_consensus_parameters();

                    // verify tx
                    let checked_tx = check_single_tx(
                        tx,
                        current_height,
                        self.tx_pool_shared_state.utxo_validation,
                        params.as_ref(),
                        &self.tx_pool_shared_state.gas_price_provider,
                        self.tx_pool_shared_state.memory_pool.get_memory().await,
                    ).await;

                    let acceptance = match checked_tx {
                        Ok(tx) => {
                            let id = tx.transaction().cached_id().expect("`Checked` tx should have cached id");
                            let txs = vec![tx];

                            // insert tx
                            let mut result = tracing::info_span!("Received tx via gossip", %id)
                                .in_scope(|| {
                                    self.tx_pool_shared_state.txpool.lock().insert(
                                        &self.tx_pool_shared_state.tx_status_sender,
                                        version,
                                        txs
                                    )
                                });

                            match result.pop() {
                                Some(Ok(_)) => {
                                    GossipsubMessageAcceptance::Accept
                                },
                                // Use similar p2p punishment rules as bitcoin
                                // https://github.com/bitcoin/bitcoin/blob/6ff0aa089c01ff3e610ecb47814ed739d685a14c/src/net_processing.cpp#L1856
                                Some(Err(TxPoolError::ConsensusValidity(_))) | Some(Err(TxPoolError::MintIsDisallowed)) => {
                                    GossipsubMessageAcceptance::Reject
                                },
                                _ => GossipsubMessageAcceptance::Ignore
                            }
                        }
                        Err(_) => {
                            GossipsubMessageAcceptance::Reject
                        }
                    };

                    // notify p2p layer about whether this tx was accepted
                    let message_info = GossipsubMessageInfo {
                        message_id,
                        peer_id,
                    };

                    let _ = self.tx_pool_shared_state.p2p.notify_gossip_transaction_validity(message_info, acceptance);

                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        // Maybe we will save and load the previous list of transactions in the future to
        // avoid losing them.
        Ok(())
    }
}

// TODO: Remove `find` and `find_one` methods from `txpool`. It is used only by GraphQL.
//  Instead, `fuel-core` can create a `DatabaseWithTxPool` that aggregates `TxPool` and
//  storage `Database` together. GraphQL will retrieve data from this `DatabaseWithTxPool` via
//  `StorageInspect` trait.
impl<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
    SharedState<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
{
    pub fn pending_number(&self) -> usize {
        self.txpool.lock().pending_number()
    }

    pub fn total_consumable_gas(&self) -> u64 {
        self.txpool.lock().consumable_gas()
    }

    pub fn remove_txs(&self, ids: Vec<(TxId, String)>) -> Vec<ArcPoolTx> {
        self.txpool.lock().remove(&self.tx_status_sender, ids)
    }

    pub fn find(&self, ids: Vec<TxId>) -> Vec<Option<TxInfo>> {
        self.txpool.lock().find(&ids)
    }

    pub fn find_one(&self, id: TxId) -> Option<TxInfo> {
        self.txpool.lock().find_one(&id)
    }

    pub fn select_transactions(&self, max_gas: u64) -> Vec<ArcPoolTx> {
        let mut guard = self.txpool.lock();
        let txs = guard.includable();
        let sorted_txs = select_transactions(txs, max_gas);

        for tx in sorted_txs.iter() {
            guard.remove_committed_tx(&tx.id());
        }
        sorted_txs
    }

    pub fn remove(&self, ids: Vec<(TxId, String)>) -> Vec<ArcPoolTx> {
        self.txpool.lock().remove(&self.tx_status_sender, ids)
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

impl<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, View, MP>
    SharedState<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<LatestView = View>,
    View: TxPoolDb,
    WasmChecker: WasmCheckerConstraint + Send + Sync,
    GasPriceProvider: GasPriceProviderConstraint + Send + Sync,
    ConsensusProvider: ConsensusParametersProvider,
    MP: MemoryPool + Send + Sync,
{
    #[tracing::instrument(name = "insert_submitted_txn", skip_all)]
    pub async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> Vec<Result<InsertionResult, TxPoolError>> {
        // verify txs
        let current_height = *self.current_height.lock();
        let (version, params) = self
            .consensus_parameters_provider
            .latest_consensus_parameters();

        let checked_txs = check_transactions(
            &txs,
            current_height,
            self.utxo_validation,
            params.as_ref(),
            &self.gas_price_provider,
            self.memory_pool.clone(),
        )
        .await;

        let mut valid_txs = vec![];

        let checked_txs: Vec<_> = checked_txs
            .into_iter()
            .map(|tx_check| match tx_check {
                Ok(tx) => {
                    valid_txs.push(tx);
                    None
                }
                Err(err) => Some(err),
            })
            .collect();

        // insert txs
        let insertion = {
            self.txpool
                .lock()
                .insert(&self.tx_status_sender, version, valid_txs)
        };

        for (ret, tx) in insertion.iter().zip(txs.into_iter()) {
            match ret {
                Ok(_) => {
                    let result = self.p2p.broadcast_transaction(tx.clone());
                    if let Err(e) = result {
                        // It can be only in the case of p2p being down or requests overloading it.
                        tracing::error!(
                            "Unable to broadcast transaction, got an {} error",
                            e
                        );
                    }
                }
                Err(_) => {}
            }
        }

        let mut insertion = insertion.into_iter();

        checked_txs
            .into_iter()
            .map(|check_result| match check_result {
                None => insertion.next().unwrap_or_else(|| {
                    Err(TxPoolError::Other(
                        anyhow::anyhow!("Insertion result is missing").to_string(),
                    ))
                }),
                Some(err) => Err(err),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct TxUpdate {
    tx_id: Bytes32,
    message: TxStatusMessage,
}

impl TxUpdate {
    pub fn new(tx_id: Bytes32, message: TxStatusMessage) -> Self {
        Self { tx_id, message }
    }

    pub fn tx_id(&self) -> &Bytes32 {
        &self.tx_id
    }

    pub fn into_msg(self) -> TxStatusMessage {
        self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxStatusMessage {
    Status(TransactionStatus),
    FailedStatus,
}

#[allow(clippy::too_many_arguments)]
pub fn new_service<
    P2P,
    Importer,
    ViewProvider,
    WasmChecker,
    GasPriceProvider,
    ConsensusProvider,
    MP,
>(
    config: Config,
    provider: ViewProvider,
    importer: Importer,
    p2p: P2P,
    wasm_checker: WasmChecker,
    current_height: BlockHeight,
    gas_price_provider: GasPriceProvider,
    consensus_parameters_provider: ConsensusProvider,
    memory_pool: MP,
) -> Service<P2P, ViewProvider, WasmChecker, GasPriceProvider, ConsensusProvider, MP>
where
    Importer: BlockImporter,
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData> + 'static,
    ViewProvider: AtomicView,
    ViewProvider::LatestView: TxPoolDb,
    WasmChecker: WasmCheckerConstraint + Send + Sync,
    GasPriceProvider: GasPriceProviderConstraint + Send + Sync,
    ConsensusProvider: ConsensusParametersProvider + Send + Sync,
    MP: MemoryPool + Send + Sync,
{
    let p2p = Arc::new(p2p);
    let gossiped_tx_stream = p2p.gossiped_transaction_events();
    let committed_block_stream = importer.block_events();
    let mut ttl_timer = tokio::time::interval(config.transaction_ttl);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let number_of_active_subscription = config.number_of_active_subscription;
    let txpool = Arc::new(ParkingMutex::new(TxPool::new(
        config.clone(),
        provider,
        wasm_checker,
    )));
    let task = Task {
        gossiped_tx_stream,
        committed_block_stream,
        tx_pool_shared_state: SharedState {
            tx_status_sender: TxStatusChange::new(
                number_of_active_subscription,
                // The connection should be closed automatically after the `SqueezedOut` event.
                // But because of slow/malicious consumers, the subscriber can still be occupied.
                // We allow the subscriber to receive the event produced by TxPool's TTL.
                // But we still want to drop subscribers after `2 * TxPool_TTL`.
                config.transaction_ttl.saturating_mul(2),
            ),
            txpool,
            p2p,
            utxo_validation: config.utxo_validation,
            current_height: Arc::new(ParkingMutex::new(current_height)),
            consensus_parameters_provider: Arc::new(consensus_parameters_provider),
            gas_price_provider: Arc::new(gas_price_provider),
            memory_pool: Arc::new(memory_pool),
        },
        ttl_timer,
    };

    Service::new(task)
}

impl<E> From<Result<TransactionStatus, E>> for TxStatusMessage {
    fn from(result: Result<TransactionStatus, E>) -> Self {
        match result {
            Ok(status) => TxStatusMessage::Status(status),
            _ => TxStatusMessage::FailedStatus,
        }
    }
}

#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod tests_p2p;
