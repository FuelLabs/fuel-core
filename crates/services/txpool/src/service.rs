use crate::{
    ports::{
        BlockImporter,
        PeerToPeer,
        TxPoolDb,
    },
    transaction_selector::select_transactions,
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
        block_importer::ImportResult,
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
            TxStatus,
        },
    },
};
use parking_lot::Mutex as ParkingMutex;
use std::sync::Arc;
use tokio::{
    sync::broadcast,
    time::MissedTickBehavior,
};
use tokio_stream::StreamExt;

pub type Service<P2P, DB> = ServiceRunner<Task<P2P, DB>>;

#[derive(Clone)]
pub struct TxStatusChange {
    status_sender: broadcast::Sender<TxStatus>,
    update_sender: broadcast::Sender<TxUpdate>,
}

impl TxStatusChange {
    pub fn new(capacity: usize) -> Self {
        let (status_sender, _) = broadcast::channel(capacity);
        let (update_sender, _) = broadcast::channel(capacity);
        Self {
            status_sender,
            update_sender,
        }
    }

    pub fn send_complete(&self, id: Bytes32, block_height: &BlockHeight) {
        tracing::info!("Transaction {id} successfully included in block {block_height}");
        let _ = self.status_sender.send(TxStatus::Completed);
        self.updated(id);
    }

    pub fn send_submitted(&self, id: Bytes32) {
        tracing::info!("Transaction {id} successfully submitted to the tx pool");
        let _ = self.status_sender.send(TxStatus::Submitted);
        self.updated(id);
    }

    pub fn send_squeezed_out(&self, id: Bytes32, reason: TxPoolError) {
        tracing::info!("Transaction {id} squeezed out because {reason}");
        let _ = self.status_sender.send(TxStatus::SqueezedOut {
            reason: reason.clone(),
        });
        let _ = self.update_sender.send(TxUpdate::squeezed_out(id, reason));
    }

    fn updated(&self, id: Bytes32) {
        let _ = self.update_sender.send(TxUpdate::updated(id));
    }
}

pub struct SharedState<P2P, DB> {
    tx_status_sender: TxStatusChange,
    txpool: Arc<ParkingMutex<TxPool<DB>>>,
    p2p: Arc<P2P>,
    consensus_params: ConsensusParameters,
}

impl<P2P, DB> Clone for SharedState<P2P, DB> {
    fn clone(&self) -> Self {
        Self {
            tx_status_sender: self.tx_status_sender.clone(),
            txpool: self.txpool.clone(),
            p2p: self.p2p.clone(),
            consensus_params: self.consensus_params,
        }
    }
}

pub struct Task<P2P, DB> {
    gossiped_tx_stream: BoxStream<TransactionGossipData>,
    committed_block_stream: BoxStream<Arc<ImportResult>>,
    shared: SharedState<P2P, DB>,
    ttl_timer: tokio::time::Interval,
}

#[async_trait::async_trait]
impl<P2P, DB> RunnableService for Task<P2P, DB>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData> + Send + Sync,
    DB: TxPoolDb,
{
    const NAME: &'static str = "TxPool";

    type SharedData = SharedState<P2P, DB>;
    type Task = Task<P2P, DB>;
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
impl<P2P, DB> RunnableTask for Task<P2P, DB>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData> + Send + Sync,
    DB: TxPoolDb,
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
                    self.shared.txpool.lock().block_update(&self.shared.tx_status_sender, &result.sealed_block);
                    should_continue = true;
                } else {
                    should_continue = false;
                }
            }

            new_transaction = self.gossiped_tx_stream.next() => {
                if let Some(GossipData { data: Some(tx), message_id, peer_id }) = new_transaction {
                    let id = tx.id(&self.shared.consensus_params);
                    let txs = vec!(Arc::new(tx));
                    let mut result = tracing::info_span!("Received tx via gossip", %id)
                        .in_scope(|| {
                            self.shared.txpool.lock().insert(
                                &self.shared.tx_status_sender,
                                &txs
                            )
                        });

                    if let Some(acceptance) = match result.pop() {
                        Some(Ok(_)) => {
                            Some(GossipsubMessageAcceptance::Accept)
                        },
                        Some(Err(_)) => {
                            Some(GossipsubMessageAcceptance::Reject)
                        }
                        _ => None
                    } {
                        let message_info = GossipsubMessageInfo {
                            message_id,
                            peer_id,
                        };
                        let _ = self.shared.p2p.notify_gossip_transaction_validity(message_info, acceptance);
                    }

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
impl<P2P, DB> SharedState<P2P, DB>
where
    DB: TxPoolDb,
{
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

    pub fn tx_status_subscribe(&self) -> broadcast::Receiver<TxStatus> {
        self.tx_status_sender.status_sender.subscribe()
    }

    pub fn tx_update_subscribe(&self) -> broadcast::Receiver<TxUpdate> {
        self.tx_status_sender.update_sender.subscribe()
    }
}

impl<P2P, DB> SharedState<P2P, DB>
where
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData>,
    DB: TxPoolDb,
{
    #[tracing::instrument(name = "insert_submitted_txn", skip_all)]
    pub fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> Vec<anyhow::Result<InsertionResult>> {
        let insert = { self.txpool.lock().insert(&self.tx_status_sender, &txs) };

        for (ret, tx) in insert.iter().zip(txs.into_iter()) {
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
        insert
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TxUpdate {
    tx_id: Bytes32,
    squeezed_out: Option<TxPoolError>,
}

impl TxUpdate {
    pub fn updated(tx_id: Bytes32) -> Self {
        Self {
            tx_id,
            squeezed_out: None,
        }
    }

    pub fn squeezed_out(tx_id: Bytes32, reason: TxPoolError) -> Self {
        Self {
            tx_id,
            squeezed_out: Some(reason),
        }
    }

    pub fn tx_id(&self) -> &Bytes32 {
        &self.tx_id
    }

    pub fn was_squeezed_out(&self) -> bool {
        self.squeezed_out.is_some()
    }

    pub fn into_squeezed_out_reason(self) -> Option<TxPoolError> {
        self.squeezed_out
    }
}

pub fn new_service<P2P, Importer, DB>(
    config: Config,
    db: DB,
    importer: Importer,
    p2p: P2P,
) -> Service<P2P, DB>
where
    Importer: BlockImporter,
    P2P: PeerToPeer<GossipedTransaction = TransactionGossipData> + 'static,
    DB: TxPoolDb + 'static,
{
    let p2p = Arc::new(p2p);
    let gossiped_tx_stream = p2p.gossiped_transaction_events();
    let committed_block_stream = importer.block_events();
    let mut ttl_timer = tokio::time::interval(config.transaction_ttl);
    ttl_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let consensus_params = config.chain_config.transaction_parameters;
    let txpool = Arc::new(ParkingMutex::new(TxPool::new(config, db)));
    let task = Task {
        gossiped_tx_stream,
        committed_block_stream,
        shared: SharedState {
            tx_status_sender: TxStatusChange::new(100),
            txpool,
            p2p,
            consensus_params,
        },
        ttl_timer,
    };

    Service::new(task)
}

#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod tests_p2p;
