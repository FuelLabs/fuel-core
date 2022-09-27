use crate::{
    Config,
    TxPool,
};
use anyhow::anyhow;
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    txpool::{
        self,
        TxPoolDb,
        TxPoolMpsc,
        TxStatusBroadcast,
    },
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

pub struct ServiceBuilder {
    sender: txpool::Sender,
    receiver: mpsc::Receiver<TxPoolMpsc>,
    config: Config,
    broadcast: broadcast::Sender<TxStatusBroadcast>,
    db: Option<Box<dyn TxPoolDb>>,
    import_block_events: Option<broadcast::Receiver<ImportBlockBroadcast>>,
}

impl Default for ServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBuilder {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(100);
        let (broadcast, _receiver) = broadcast::channel(100);
        Self {
            sender: txpool::Sender::new(sender),
            receiver,
            db: None,
            broadcast,
            config: Default::default(),
            import_block_events: None,
        }
    }

    pub fn sender(&self) -> &txpool::Sender {
        &self.sender
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TxStatusBroadcast> {
        self.broadcast.subscribe()
    }

    pub fn db(&mut self, db: Box<dyn TxPoolDb>) -> &mut Self {
        self.db = Some(db);
        self
    }

    pub fn import_block_event(
        &mut self,
        import_block_event: broadcast::Receiver<ImportBlockBroadcast>,
    ) -> &mut Self {
        self.import_block_events = Some(import_block_event);
        self
    }

    pub fn config(&mut self, config: Config) -> &mut Self {
        self.config = config;
        self
    }

    pub fn build(self) -> anyhow::Result<Service> {
        if self.db.is_none() || self.import_block_events.is_none() {
            return Err(anyhow!("One of context items are not set"))
        }
        let service = Service::new(
            self.sender,
            self.broadcast.clone(),
            Context {
                receiver: self.receiver,
                broadcast: self.broadcast,
                db: Arc::new(self.db.unwrap()),
                import_block_events: self.import_block_events.unwrap(),
                config: self.config,
            },
        )?;
        Ok(service)
    }
}

pub struct Context {
    pub config: Config,
    pub broadcast: broadcast::Sender<TxStatusBroadcast>,
    pub db: Arc<Box<dyn TxPoolDb>>,
    pub receiver: mpsc::Receiver<TxPoolMpsc>,
    pub import_block_events: broadcast::Receiver<ImportBlockBroadcast>,
}

impl Context {
    pub async fn run(mut self) -> Self {
        let txpool = Arc::new(RwLock::new(TxPool::new(self.config.clone())));

        loop {
            tokio::select! {
                event = self.receiver.recv() => {
                    if matches!(event,Some(TxPoolMpsc::Stop) | None) {
                        break;
                    }
                    let broadcast = self.broadcast.clone();
                    let db = self.db.clone();
                    let txpool = txpool.clone();

                    // This is little bit risky but we can always add semaphore to limit number of requests.
                    tokio::spawn( async move {
                        let txpool = txpool.as_ref();
                    match event.unwrap() {
                        TxPoolMpsc::Includable { response } => {
                            let _ = response.send(TxPool::includable(txpool).await);
                        }
                        TxPoolMpsc::Insert { txs, response } => {
                            let _ = response.send(TxPool::insert(txpool,db.as_ref().as_ref(),broadcast, txs).await);
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
                            let _ = response.send(TxPool::remove(txpool,broadcast,&ids).await);
                        }
                        TxPoolMpsc::Stop => {}
                    }});
                }
                block_updated = self.import_block_events.recv() => {
                    if let Ok(block_updated) = block_updated {

                        match block_updated {
                            ImportBlockBroadcast::PendingFuelBlockImported { block } => {
                                let txpool = txpool.clone();
                                TxPool::block_update(txpool.as_ref(), block).await
                                // TODO: Should this be done in a separate task? Like this:
                                // tokio::spawn( async move {
                                //     TxPool::block_update(txpool.as_ref(), block).await
                                // });
                            },
                            ImportBlockBroadcast::SealedFuelBlockImported { block: _, is_created_by_self: _ } => {
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
    sender: txpool::Sender,
    broadcast: broadcast::Sender<TxStatusBroadcast>,
    join: Mutex<Option<JoinHandle<Context>>>,
    context: Arc<Mutex<Option<Context>>>,
}

impl Service {
    pub fn new(
        sender: txpool::Sender,
        broadcast: broadcast::Sender<TxStatusBroadcast>,
        context: Context,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            sender,
            broadcast,
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
            let _ = self.sender.send(TxPoolMpsc::Stop).await;
            let context = self.context.clone();
            Some(tokio::spawn(async move {
                let ret = join_handle.await;
                *context.lock().await = ret.ok();
            }))
        } else {
            None
        }
    }

    pub fn subscribe_ch(&self) -> broadcast::Receiver<TxStatusBroadcast> {
        self.broadcast.subscribe()
    }

    pub fn sender(&self) -> &txpool::Sender {
        &self.sender
    }
}

#[cfg(any(test))]
pub mod tests {
    use super::*;
    use crate::MockDb;
    use fuel_core_interfaces::{
        common::fuel_tx::TransactionBuilder,
        txpool::{
            Error as TxpoolError,
            TxStatus,
        },
    };
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_start_stop() {
        let config = Config::default();
        let db = Box::new(MockDb::default());
        let (bs, _br) = broadcast::channel(10);

        let mut builder = ServiceBuilder::new();
        builder
            .config(config)
            .db(db)
            .import_block_event(bs.subscribe());
        let service = builder.build().unwrap();

        assert!(service.start().await.is_ok(), "start service");

        // Double start will return false.
        assert!(service.start().await.is_err(), "double start should fail");

        let stop_handle = service.stop().await;
        assert!(stop_handle.is_some());
        let _ = stop_handle.unwrap().await;

        assert!(service.start().await.is_ok(), "Should start again");
    }

    #[tokio::test]
    async fn test_filter_by_negative() {
        let config = Config::default();
        let db = Box::new(MockDb::default());
        let (_bs, br) = broadcast::channel(10);

        let mut builder = ServiceBuilder::new();
        builder.config(config).db(db).import_block_event(br);
        let service = builder.build().unwrap();
        service.start().await.ok();

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(30)
                .finalize(),
        );

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Insert {
                txs: vec![tx1.clone(), tx2.clone()],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 2, "Should be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::FilterByNegative {
                ids: vec![tx1.id(), tx2.id(), tx3.id()],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 1, "Should be len 1:{:?}", out);
        assert_eq!(out[0], tx3.id(), "Found tx id match{:?}", out);
        service.stop().await.unwrap().await.unwrap();
    }

    #[tokio::test]
    async fn test_find() {
        let config = Config::default();
        let db = Box::new(MockDb::default());
        let (_bs, br) = broadcast::channel(10);

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .finalize(),
        );
        let tx3 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(30)
                .finalize(),
        );

        let mut builder = ServiceBuilder::new();
        builder.config(config).db(db).import_block_event(br);
        let service = builder.build().unwrap();
        service.start().await.ok();

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Insert {
                txs: vec![tx1.clone(), tx2.clone()],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 2, "Should be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Find {
                ids: vec![tx1.id(), tx3.id()],
                response,
            })
            .await;
        let out = receiver.await.unwrap();
        assert_eq!(out.len(), 2, "Should be len 2:{:?}", out);
        assert!(out[0].is_some(), "Tx1 should be some:{:?}", out);
        let id = out[0].as_ref().unwrap().id();
        assert_eq!(id, tx1.id(), "Found tx id match{:?}", out);
        assert!(out[1].is_none(), "Tx3 should not be found:{:?}", out);
        service.stop().await.unwrap().await.unwrap();
    }

    #[tokio::test]
    async fn simple_insert_removal_subscription() {
        let config = Config::default();
        let db = Box::new(MockDb::default());
        let (_bs, br) = broadcast::channel(10);

        let tx1 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(10)
                .finalize(),
        );
        let tx2 = Arc::new(
            TransactionBuilder::script(vec![], vec![])
                .gas_price(20)
                .finalize(),
        );

        let mut builder = ServiceBuilder::new();
        builder.config(config).db(db).import_block_event(br);
        let service = builder.build().unwrap();
        service.start().await.ok();

        let mut subscribe = service.subscribe_ch();

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Insert {
                txs: vec![tx1.clone(), tx2.clone()],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);

        // we are sure that included tx are already broadcasted.
        assert_eq!(
            subscribe.try_recv(),
            Ok(TxStatusBroadcast {
                tx: tx1.clone(),
                status: TxStatus::Submitted,
            }),
            "First added should be tx1"
        );
        assert_eq!(
            subscribe.try_recv(),
            Ok(TxStatusBroadcast {
                tx: tx2.clone(),
                status: TxStatus::Submitted,
            }),
            "Second added should be tx2"
        );

        // remove them
        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Remove {
                ids: vec![tx1.id(), tx2.id()],
                response,
            })
            .await;
        let _rem = receiver.await.unwrap();

        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), subscribe.recv())
                .await,
            Ok(Ok(TxStatusBroadcast {
                tx: tx1,
                status: TxStatus::SqueezedOut {
                    reason: TxpoolError::Removed
                }
            })),
            "First removed should be tx1"
        );

        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), subscribe.recv())
                .await,
            Ok(Ok(TxStatusBroadcast {
                tx: tx2,
                status: TxStatus::SqueezedOut {
                    reason: TxpoolError::Removed
                }
            })),
            "Second removed should be tx2"
        );
    }
}
