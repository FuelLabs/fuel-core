use crate::{
    ports::{
        BlockImport,
        PeerToPeer,
        TxPoolDb,
    },
    transaction_selector::select_transactions,
    Config,
    Error as TxPoolError,
    TxPool,
};
use anyhow::anyhow;
use fuel_core_types::{
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::Bytes32,
    services::{
        p2p::{
            GossipData,
            TransactionGossipData,
        },
        txpool::{
            ArcPoolTx,
            InsertionResult,
            Result as TxPoolResult,
            TxInfo,
            TxStatus,
        },
    },
};
use std::sync::Arc;
use tokio::{
    sync::{
        broadcast,
        mpsc,
        oneshot,
        Mutex,
        RwLock,
    },
    task::JoinHandle,
};

type PeerToPeerForTx = Box<dyn PeerToPeer<GossipedTransaction = TransactionGossipData>>;

pub struct ServiceBuilder {
    config: Config,
    db: Option<Arc<dyn TxPoolDb>>,
    tx_status_sender: Option<TxStatusChange>,
    importer: Option<Box<dyn BlockImport>>,
    p2p_port: Option<PeerToPeerForTx>,
}

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
    pub fn send_complete(&self, id: Bytes32) {
        let _ = self.status_sender.send(TxStatus::Completed);
        self.updated(id);
    }

    pub fn send_submitted(&self, id: Bytes32) {
        let _ = self.status_sender.send(TxStatus::Submitted);
        self.updated(id);
    }

    pub fn send_squeezed_out(&self, id: Bytes32, reason: TxPoolError) {
        let _ = self.status_sender.send(TxStatus::SqueezedOut {
            reason: reason.clone(),
        });
        let _ = self.update_sender.send(TxUpdate::squeezed_out(id, reason));
    }

    fn updated(&self, id: Bytes32) {
        let _ = self.update_sender.send(TxUpdate::updated(id));
    }
}

impl Default for ServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBuilder {
    pub fn new() -> Self {
        Self {
            config: Default::default(),
            db: None,
            tx_status_sender: None,
            importer: None,
            p2p_port: None,
        }
    }

    pub fn tx_status_subscribe(&self) -> broadcast::Receiver<TxStatus> {
        self.tx_status_sender
            .as_ref()
            .unwrap()
            .status_sender
            .subscribe()
    }

    pub fn tx_change_subscribe(&self) -> broadcast::Receiver<TxUpdate> {
        self.tx_status_sender
            .as_ref()
            .unwrap()
            .update_sender
            .subscribe()
    }

    pub fn db(&mut self, db: Arc<dyn TxPoolDb>) -> &mut Self {
        self.db = Some(db);
        self
    }

    pub fn tx_status_sender(&mut self, tx_status_sender: TxStatusChange) -> &mut Self {
        self.tx_status_sender = Some(tx_status_sender);
        self
    }

    pub fn p2p_port(&mut self, p2p_port: PeerToPeerForTx) -> &mut Self {
        self.p2p_port = Some(p2p_port);
        self
    }

    pub fn importer(&mut self, importer: Box<dyn BlockImport>) -> &mut Self {
        self.importer = Some(importer);
        self
    }

    pub fn config(&mut self, config: Config) -> &mut Self {
        self.config = config;
        self
    }

    pub fn build(self) -> anyhow::Result<Service> {
        if self.db.is_none()
            || self.importer.is_none()
            || self.p2p_port.is_none()
            || self.tx_status_sender.is_none()
        {
            return Err(anyhow!("One of context items are not set"))
        }

        let (sender, receiver) = mpsc::channel(100);

        let service = Service::new(
            sender,
            self.tx_status_sender.clone().unwrap(),
            Context {
                config: self.config,
                db: self.db.unwrap(),
                txpool_receiver: receiver,
                tx_status_sender: self.tx_status_sender.unwrap(),
                importer: self.importer.unwrap(),
                p2p_port: self.p2p_port.unwrap(),
            },
        )?;
        Ok(service)
    }
}

pub struct Context {
    config: Config,
    db: Arc<dyn TxPoolDb>,
    txpool_receiver: mpsc::Receiver<TxPoolMpsc>,
    tx_status_sender: TxStatusChange,
    importer: Box<dyn BlockImport>,
    p2p_port: PeerToPeerForTx,
}

impl Context {
    pub async fn run(mut self) -> Self {
        let txpool = Arc::new(RwLock::new(TxPool::new(self.config.clone())));

        loop {
            tokio::select! {
                new_transaction = self.p2p_port.next_gossiped_transaction() => {
                    let txpool = txpool.clone();
                    let db = self.db.clone();
                    let tx_status_sender = self.tx_status_sender.clone();

                    tokio::spawn( async move {
                        let txpool = txpool.as_ref();
                        if let GossipData { data: Some(tx), .. } = new_transaction {
                            let txs = vec!(Arc::new(tx));
                            TxPool::insert(txpool, db.as_ref(), &tx_status_sender, &txs).await;
                        }
                    });
                }

                event = self.txpool_receiver.recv() => {
                    if matches!(event, Some(TxPoolMpsc::Stop) | None) {
                        break;
                    }
                    let txpool = txpool.clone();
                    let db = self.db.clone();
                    let tx_status_sender = self.tx_status_sender.clone();

                    // This is little bit risky but we can always add semaphore to limit number of requests.
                        let txpool = txpool.as_ref();
                    match event.unwrap() {
                        TxPoolMpsc::PendingNumber { response } => {
                            let _ = response.send(TxPool::pending_number(txpool).await);
                        }
                        TxPoolMpsc::ConsumableGas { response } => {
                            let _ = response.send(TxPool::consumable_gas(txpool).await);
                        }
                        TxPoolMpsc::Includable { response } => {
                            let _ = response.send(TxPool::includable(txpool).await);
                        }
                        TxPoolMpsc::Insert { txs, response } => {
                            let insert = TxPool::insert(txpool, db.as_ref(), &tx_status_sender, &txs).await;
                            for (ret, tx) in insert.iter().zip(txs.into_iter()) {
                                match ret {
                                    Ok(_) => {
                                        let _ = self.p2p_port.broadcast_transaction(tx.clone()).await;
                                    }
                                    Err(_) => {}
                                }
                            }
                            let _ = response.send(insert);
                        }
                        TxPoolMpsc::Find { ids, response } => {
                            let _ = response.send(TxPool::find(txpool,&ids).await);
                        }
                        TxPoolMpsc::FindOne { id, response } => {
                            let _ = response.send(TxPool::find_one(txpool,&id).await);
                        }
                        TxPoolMpsc::FindDependent { ids, response } => {
                            let _ = response.send(TxPool::find_dependent(txpool,&ids).await);
                        }
                        TxPoolMpsc::FilterByNegative { ids, response } => {
                            let _ = response.send(TxPool::filter_by_negative(txpool,&ids).await);
                        }
                        TxPoolMpsc::Remove { ids, response } => {
                            let _ = response.send(TxPool::remove(txpool, &tx_status_sender ,&ids).await);
                        }
                        TxPoolMpsc::Stop => {}
                    };
                }

                block = self.importer.next_block() => {
                    let txpool = txpool.clone();
                    TxPool::block_update(txpool.as_ref(), &self.tx_status_sender, block).await;
                }
            }
        }
        self
    }
}

pub struct Service {
    txpool_sender: mpsc::Sender<TxPoolMpsc>,
    tx_status_sender: TxStatusChange,
    join: Mutex<Option<JoinHandle<Context>>>,
    context: Arc<Mutex<Option<Context>>>,
}

impl Service {
    fn new(
        txpool_sender: mpsc::Sender<TxPoolMpsc>,
        tx_status_sender: TxStatusChange,
        context: Context,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            txpool_sender,
            tx_status_sender,
            join: Mutex::new(None),
            context: Arc::new(Mutex::new(Some(context))),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let mut join = self.join.lock().await;
        if join.is_none() {
            if let Some(context) = self.context.lock().await.take() {
                *join = Some(tokio::spawn(async { context.run().await }));
                Ok(())
            } else {
                Err(anyhow!("Starting TxPool service that is stopping"))
            }
        } else {
            Err(anyhow!("Service TxPool is already started"))
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let mut join = self.join.lock().await;
        let join_handle = join.take();

        if let Some(join_handle) = join_handle {
            let _ = self.txpool_sender.send(TxPoolMpsc::Stop).await;
            let context = self.context.clone();
            Some(tokio::spawn(async move {
                let ret = join_handle.await;
                *context.lock().await = ret.ok();
            }))
        } else {
            None
        }
    }

    pub fn tx_status_subscribe(&self) -> broadcast::Receiver<TxStatus> {
        self.tx_status_sender.status_sender.subscribe()
    }

    pub fn tx_update_subscribe(&self) -> broadcast::Receiver<TxUpdate> {
        self.tx_status_sender.update_sender.subscribe()
    }
}

// TODO: Return `TxPoolResult`
// TODO: Remove `find` and `find_one` methods from `txpool`. It is used only by GraphQL.
//  Instead, `fuel-core` can create a `DatabaseWithTxPool` that aggregates `TxPool` and
//  storage `Database` together. GraphQL will retrieve data from this `DatabaseWithTxPool` via
//  `StorageInspect` trait.
impl Service {
    pub async fn pending_number(&self) -> anyhow::Result<usize> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::PendingNumber { response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn total_consumable_gas(&self) -> anyhow::Result<u64> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::ConsumableGas { response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn remove_txs(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::Remove { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> anyhow::Result<Vec<anyhow::Result<InsertionResult>>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::Insert { txs, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<Option<TxInfo>>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::Find { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find_one(&self, id: TxId) -> anyhow::Result<Option<TxInfo>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::FindOne { id, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find_dependent(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::FindDependent { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn filter_by_negative(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<TxId>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::FilterByNegative { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn select_transactions(
        &self,
        max_gas: u64,
    ) -> TxPoolResult<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::Includable { response })
            .await
            .map_err(|e| TxPoolError::Other(e.to_string()))?;
        let txs = receiver
            .await
            .map_err(|e| TxPoolError::Other(e.to_string()))?;
        Ok(select_transactions(txs, max_gas))
    }

    pub async fn remove(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.txpool_sender
            .send(TxPoolMpsc::Remove { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }
}

/// RPC commands that can be sent to the TxPool through an MPSC channel.
/// Responses are returned using `response` oneshot channel.
#[derive(Debug)]
enum TxPoolMpsc {
    /// The number of pending transactions in the pool.
    PendingNumber { response: oneshot::Sender<usize> },
    /// The amount of gas in all includable transactions combined
    ConsumableGas { response: oneshot::Sender<u64> },
    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    Includable {
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// import list of transaction into txpool. All needed parents need to be known
    /// and parent->child order should be enforced in Vec, we will not do that check inside
    /// txpool and will just drop child and include only parent. Additional restrain is that
    /// child gas_price needs to be lower then parent gas_price. Transaction can be received
    /// from p2p **RespondTransactions** or from userland. Because of userland we are returning
    /// error for every insert for better user experience.
    Insert {
        txs: Vec<Arc<Transaction>>,
        response: oneshot::Sender<Vec<anyhow::Result<InsertionResult>>>,
    },
    /// find all tx by their hash
    Find {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<Option<TxInfo>>>,
    },
    /// find one tx by its hash
    FindOne {
        id: TxId,
        response: oneshot::Sender<Option<TxInfo>>,
    },
    /// find all dependent tx and return them with requested dependencies in one list sorted by Price.
    FindDependent {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// remove transaction from pool needed on user demand. Low priority
    Remove {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// Iterate over `hashes` and return all hashes that we don't have.
    /// Needed when we receive list of new hashed from peer with
    /// **BroadcastTransactionHashes**, so txpool needs to return
    /// tx that we don't have, and request them from that particular peer.
    FilterByNegative {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<TxId>>,
    },
    /// stop txpool
    Stop,
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

#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod tests_p2p;
