use crate::{
    ports::{
        BlockImporter,
        PeerToPeer,
        TxPoolDb,
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

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::{
    fuel_tx::{
        ConsensusParameters,
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        p2p::{
            GossipData,
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            TransactionGossipData,
        },
        txpool::{
            ArcPoolTx,
            Error,
            InsertionResult,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};

use anyhow::anyhow;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::services::block_importer::SharedImportResult;
use parking_lot::Mutex as ParkingMutex;
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::broadcast,
    time::MissedTickBehavior,
};
use tokio_stream::StreamExt;
use update_sender::UpdateSender;

use self::update_sender::{
    MpscChannel,
    TxStatusStream,
};

mod update_sender;

pub type Service<P2P, DB> = ServiceRunner<Task<P2P, DB>>;

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
        message: impl Into<TxStatusMessage>,
    ) {
        tracing::info!("Transaction {id} successfully included in block {block_height}");
        self.update_sender.send(TxUpdate::new(id, message.into()));
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

pub struct SharedState<P2P, ViewProvider> {
    tx_status_sender: TxStatusChange,
    txpool: Arc<ParkingMutex<TxPool<ViewProvider>>>,
    p2p: Arc<P2P>,
    consensus_params: ConsensusParameters,
    current_height: Arc<ParkingMutex<BlockHeight>>,
    config: Config,
}

impl<P2P, ViewProvider> Clone for SharedState<P2P, ViewProvider> {
    fn clone(&self) -> Self {
        Self {
            tx_status_sender: self.tx_status_sender.clone(),
            txpool: self.txpool.clone(),
            p2p: self.p2p.clone(),
            consensus_params: self.consensus_params.clone(),
            current_height: self.current_height.clone(),
            config: self.config.clone(),
        }
    }
}

pub struct Task<P2P, ViewProvider> {
    gossiped_tx_stream: BoxStream<TransactionGossipData>,
    committed_block_stream: BoxStream<SharedImportResult>,
    shared: SharedState<P2P, ViewProvider>,
    ttl_timer: tokio::time::Interval,
}

#[async_trait::async_trait]
impl<P2P, ViewProvider, View> RunnableService for Task<P2P, ViewProvider>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<View = View>,
    View: TxPoolDb,
{
    const NAME: &'static str = "TxPool";

    type SharedData = SharedState<P2P, ViewProvider>;
    type Task = Task<P2P, ViewProvider>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared.clone()
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
impl<P2P, ViewProvider, View> RunnableTask for Task<P2P, ViewProvider>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<View = View>,
    View: TxPoolDb,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;

        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                should_continue = false;
            }

            _ = self.ttl_timer.tick() => {
                let removed = self.shared.txpool.lock().prune_old_txs();
                for tx in removed {
                    self.shared.tx_status_sender.send_squeezed_out(tx.id(), Error::TTLReason);
                }

                should_continue = true
            }

            result = self.committed_block_stream.next() => {
                if let Some(result) = result {
                    let new_height = *result
                        .sealed_block
                        .entity.header().height();

                    let block = &result
                        .sealed_block
                        .entity;
                    {
                        let mut lock = self.shared.txpool.lock();
                        lock.block_update(
                            &self.shared.tx_status_sender,
                            block,
                            &result.tx_status,
                        );
                        *self.shared.current_height.lock() = new_height;
                    }
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_transaction = self.gossiped_tx_stream.next() => {
                if let Some(GossipData { data: Some(tx), message_id, peer_id }) = new_transaction {
                    let id = tx.id(&self.shared.consensus_params.chain_id);
                    let current_height = *self.shared.current_height.lock();

                    // verify tx
                    let checked_tx = check_single_tx(tx, current_height, &self.shared.config).await;

                    let acceptance = match checked_tx {
                        Ok(tx) => {
                            let txs = vec![tx];

                            // insert tx
                            let mut result = tracing::info_span!("Received tx via gossip", %id)
                                .in_scope(|| {
                                    self.shared.txpool.lock().insert(
                                        &self.shared.tx_status_sender,
                                        txs
                                    )
                                });

                            match result.pop() {
                                Some(Ok(_)) => {
                                    GossipsubMessageAcceptance::Accept
                                },
                                // Use similar p2p punishment rules as bitcoin
                                // https://github.com/bitcoin/bitcoin/blob/6ff0aa089c01ff3e610ecb47814ed739d685a14c/src/net_processing.cpp#L1856
                                Some(Err(Error::ConsensusValidity(_))) | Some(Err(Error::MintIsDisallowed)) => {
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

                    let _ = self.shared.p2p.notify_gossip_transaction_validity(message_info, acceptance);

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
impl<P2P, ViewProvider> SharedState<P2P, ViewProvider> {
    pub fn pending_number(&self) -> usize {
        self.txpool.lock().pending_number()
    }

    pub fn total_consumable_gas(&self) -> u64 {
        self.txpool.lock().consumable_gas()
    }

    pub fn remove_txs(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.txpool.lock().remove(&self.tx_status_sender, &ids)
    }

    pub fn find(&self, ids: Vec<TxId>) -> Vec<Option<TxInfo>> {
        self.txpool.lock().find(&ids)
    }

    pub fn find_one(&self, id: TxId) -> Option<TxInfo> {
        self.txpool.lock().find_one(&id)
    }

    pub fn find_dependent(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.txpool.lock().find_dependent(&ids)
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

    pub fn remove(&self, ids: Vec<TxId>) -> Vec<ArcPoolTx> {
        self.txpool.lock().remove(&self.tx_status_sender, &ids)
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
}

impl<P2P, ViewProvider, View> SharedState<P2P, ViewProvider>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    ViewProvider: AtomicView<View = View>,
    View: TxPoolDb,
{
    #[tracing::instrument(name = "insert_submitted_txn", skip_all)]
    pub async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> Vec<Result<InsertionResult, Error>> {
        // verify txs
        let current_height = *self.current_height.lock();

        let checked_txs = check_transactions(&txs, current_height, &self.config).await;

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
        let insertion = { self.txpool.lock().insert(&self.tx_status_sender, valid_txs) };

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
                    unreachable!(
                        "the number of inserted txs matches the number of `None` results"
                    )
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

pub fn new_service<P2P, Importer, ViewProvider>(
    config: Config,
    provider: ViewProvider,
    importer: Importer,
    p2p: P2P,
    current_height: BlockHeight,
) -> Service<P2P, ViewProvider>
where
    Importer: BlockImporter,
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData> + 'static,
    ViewProvider: AtomicView,
    ViewProvider::View: TxPoolDb,
{
    let p2p = Arc::new(p2p);
    let gossiped_tx_stream = p2p.gossiped_transaction_events();
    let committed_block_stream = importer.block_events();
    let mut ttl_timer = tokio::time::interval(config.transaction_ttl);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let consensus_params = config.chain_config.consensus_parameters.clone();
    let number_of_active_subscription = config.number_of_active_subscription;
    let txpool = Arc::new(ParkingMutex::new(TxPool::new(config.clone(), provider)));
    let task = Task {
        gossiped_tx_stream,
        committed_block_stream,
        shared: SharedState {
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
            consensus_params,
            current_height: Arc::new(ParkingMutex::new(current_height)),
            config,
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
