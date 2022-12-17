use crate::{
    Config,
    Error as TxPoolError,
    TxPool,
};
use anyhow::anyhow;
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    p2p::{
        GossipData,
        P2pRequestEvent,
        TransactionBroadcast,
        TransactionGossipData,
    },
    txpool::{
        self,
        TxPoolDb,
        TxPoolMpsc,
    },
};
use fuel_core_types::{
    fuel_types::Bytes32,
    services::txpool::TxStatus,
};
use std::sync::Arc;
use tokio::{
    sync::{
        broadcast,
        mpsc,
        Mutex,
        RwLock,
    },
    task::JoinHandle,
};
use tracing::error;

pub struct ServiceBuilder {
    config: Config,
    db: Option<Box<dyn TxPoolDb>>,
    txpool_sender: Option<txpool::Sender>,
    txpool_receiver: Option<mpsc::Receiver<TxPoolMpsc>>,
    tx_status_sender: Option<TxStatusChange>,
    import_block_receiver: Option<broadcast::Receiver<ImportBlockBroadcast>>,
    incoming_tx_receiver: Option<broadcast::Receiver<TransactionGossipData>>,
    network_sender: Option<mpsc::Sender<P2pRequestEvent>>,
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
            txpool_sender: None,
            txpool_receiver: None,
            tx_status_sender: None,
            import_block_receiver: None,
            incoming_tx_receiver: None,
            network_sender: None,
        }
    }

    pub fn sender(&self) -> &txpool::Sender {
        self.txpool_sender.as_ref().unwrap()
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

    pub fn db(&mut self, db: Box<dyn TxPoolDb>) -> &mut Self {
        self.db = Some(db);
        self
    }

    pub fn txpool_sender(&mut self, txpool_sender: txpool::Sender) -> &mut Self {
        self.txpool_sender = Some(txpool_sender);
        self
    }

    pub fn txpool_receiver(
        &mut self,
        txpool_receiver: mpsc::Receiver<TxPoolMpsc>,
    ) -> &mut Self {
        self.txpool_receiver = Some(txpool_receiver);
        self
    }

    pub fn tx_status_sender(&mut self, tx_status_sender: TxStatusChange) -> &mut Self {
        self.tx_status_sender = Some(tx_status_sender);
        self
    }

    pub fn incoming_tx_receiver(
        &mut self,
        incoming_tx_receiver: broadcast::Receiver<TransactionGossipData>,
    ) -> &mut Self {
        self.incoming_tx_receiver = Some(incoming_tx_receiver);
        self
    }

    pub fn network_sender(
        &mut self,
        network_sender: mpsc::Sender<P2pRequestEvent>,
    ) -> &mut Self {
        self.network_sender = Some(network_sender);
        self
    }

    pub fn import_block_event(
        &mut self,
        import_block_receiver: broadcast::Receiver<ImportBlockBroadcast>,
    ) -> &mut Self {
        self.import_block_receiver = Some(import_block_receiver);
        self
    }

    pub fn config(&mut self, config: Config) -> &mut Self {
        self.config = config;
        self
    }

    pub fn build(self) -> anyhow::Result<Service> {
        if self.db.is_none()
            || self.import_block_receiver.is_none()
            || self.incoming_tx_receiver.is_none()
            || self.txpool_sender.is_none()
            || self.tx_status_sender.is_none()
            || self.txpool_receiver.is_none()
            || self.network_sender.is_none()
        {
            return Err(anyhow!("One of context items are not set"))
        }

        let service = Service::new(
            self.txpool_sender.unwrap(),
            self.tx_status_sender.clone().unwrap(),
            Context {
                config: self.config,
                db: Arc::new(self.db.unwrap()),
                txpool_receiver: self.txpool_receiver.unwrap(),
                tx_status_sender: self.tx_status_sender.unwrap(),
                import_block_receiver: self.import_block_receiver.unwrap(),
                incoming_tx_receiver: self.incoming_tx_receiver.unwrap(),
                network_sender: self.network_sender.unwrap(),
            },
        )?;
        Ok(service)
    }
}

pub struct Context {
    pub config: Config,
    pub db: Arc<Box<dyn TxPoolDb>>,
    pub txpool_receiver: mpsc::Receiver<TxPoolMpsc>,
    pub tx_status_sender: TxStatusChange,
    pub import_block_receiver: broadcast::Receiver<ImportBlockBroadcast>,
    pub incoming_tx_receiver: broadcast::Receiver<TransactionGossipData>,
    pub network_sender: mpsc::Sender<P2pRequestEvent>,
}

impl Context {
    pub async fn run(mut self) -> Self {
        let txpool = Arc::new(RwLock::new(TxPool::new(self.config.clone())));

        loop {
            tokio::select! {
                new_transaction = self.incoming_tx_receiver.recv() => {
                    if new_transaction.is_err() {
                        error!("Incoming tx receiver channel closed unexpectedly; shutting down transaction pool service.");
                        break;
                    }

                    let txpool = txpool.clone();
                    let db = self.db.clone();
                    let tx_status_sender = self.tx_status_sender.clone();

                    tokio::spawn( async move {
                        let txpool = txpool.as_ref();
                        if let GossipData { data: Some(TransactionBroadcast::NewTransaction ( tx )), .. } =  new_transaction.unwrap() {
                            let txs = vec!(Arc::new(tx));
                            TxPool::insert(txpool, db.as_ref().as_ref(), &tx_status_sender, &txs).await;
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

                    let network_sender = self.network_sender.clone();

                    // This is little bit risky but we can always add semaphore to limit number of requests.
                    tokio::spawn( async move {
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
                            let insert = TxPool::insert(txpool, db.as_ref().as_ref(), &tx_status_sender, &txs).await;
                            for (ret, tx) in insert.iter().zip(txs.into_iter()) {
                                match ret {
                                    Ok(_) => {
                                        let _ = network_sender.send(P2pRequestEvent::BroadcastNewTransaction {
                                            transaction: tx.clone(),
                                        }).await;
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
                    }});
                }

                block_updated = self.import_block_receiver.recv() => {
                  if let Ok(block_updated) = block_updated {
                        match block_updated {
                            ImportBlockBroadcast::PendingFuelBlockImported { block } => {
                                let txpool = txpool.clone();
                                TxPool::block_update(txpool.as_ref(), &self.tx_status_sender, block).await
                                // TODO: Should this be done in a separate task? Like this:
                                // tokio::spawn( async move {
                                //     TxPool::block_update(txpool.as_ref(), block).await
                                // });
                            },
                            ImportBlockBroadcast::SealedBlockImported { block: _, is_created_by_self: _ } => {
                                // TODO: what to do with sealed blocks?
                                todo!("Sealed block");
                            }
                        };
                    }
                }
            }
        }
        self
    }
}

pub struct Service {
    txpool_sender: txpool::Sender,
    tx_status_sender: TxStatusChange,
    join: Mutex<Option<JoinHandle<Context>>>,
    context: Arc<Mutex<Option<Context>>>,
}

impl Service {
    pub fn new(
        txpool_sender: txpool::Sender,
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

    pub fn sender(&self) -> &txpool::Sender {
        &self.txpool_sender
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

#[cfg(test)]
pub mod test_helpers;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod tests_p2p;
