use crate::{interface::Interface, Config};
use fuel_core_interfaces::block_importer::ImportBlockBroadcast;
use fuel_core_interfaces::txpool::{TxPoolDb, TxPoolMpsc, TxStatusBroadcast};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::warn;

pub struct Service {
    interface: Arc<Interface>,
    sender: mpsc::Sender<TxPoolMpsc>,
    broadcast: broadcast::Sender<TxStatusBroadcast>,
    join: Mutex<Option<JoinHandle<mpsc::Receiver<TxPoolMpsc>>>>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<TxPoolMpsc>>>>,
}

impl Service {
    pub fn new(db: Box<dyn TxPoolDb>, config: Config) -> Result<Self, anyhow::Error> {
        let (sender, receiver) = mpsc::channel(100);
        let (broadcast, _receiver) = broadcast::channel(100);
        Ok(Self {
            interface: Arc::new(Interface::new(db, broadcast.clone(), config)),
            sender,
            broadcast,
            join: Mutex::new(None),
            receiver: Arc::new(Mutex::new(Some(receiver))),
        })
    }

    pub async fn start(&self, new_block: broadcast::Receiver<ImportBlockBroadcast>) -> bool {
        let mut join = self.join.lock().await;
        if join.is_none() {
            if let Some(receiver) = self.receiver.lock().await.take() {
                let interface = self.interface.clone();
                *join = Some(tokio::spawn(async {
                    Interface::run(interface, new_block, receiver).await
                }));
                return true;
            } else {
                warn!("Starting TxPool service that is stopping");
            }
        } else {
            warn!("Service TxPool is already started");
        }
        false
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let mut join = self.join.lock().await;
        let join_handle = join.take();
        if let Some(join_handle) = join_handle {
            let _ = self.sender.send(TxPoolMpsc::Stop).await;
            let receiver = self.receiver.clone();
            Some(tokio::spawn(async move {
                let ret = join_handle.await;
                *receiver.lock().await = ret.ok();
            }))
        } else {
            None
        }
    }

    pub fn subscribe_ch(&self) -> broadcast::Receiver<TxStatusBroadcast> {
        self.broadcast.subscribe()
    }

    pub fn sender(&self) -> &mpsc::Sender<TxPoolMpsc> {
        &self.sender
    }
}

#[cfg(any(test))]
pub mod tests {

    use super::*;
    use fuel_core_interfaces::db::helpers::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_start_stop() {
        let config = Config::default();
        let db = Box::new(DummyDb::filled());
        let (bs, _br) = broadcast::channel(10);

        let service = Service::new(db, config).unwrap();
        assert!(service.start(bs.subscribe()).await, "start service");

        //double start will return false
        assert!(
            !service.start(bs.subscribe()).await,
            "double start should fail"
        );

        let stop_handle = service.stop().await;
        assert!(stop_handle.is_some());
        let _ = stop_handle.unwrap().await;

        assert!(service.start(bs.subscribe()).await, "Should start again");
    }

    #[tokio::test]
    async fn test_filter_by_negative() {
        let config = Config::default();
        let db = Box::new(DummyDb::filled());
        let (_bs, br) = broadcast::channel(10);

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx3_hash = *TX_ID3;

        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let service = Service::new(db, config).unwrap();
        service.start(br).await;

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Insert {
                txs: vec![tx1, tx2],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::FilterByNegative {
                ids: vec![tx1_hash, tx3_hash],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 1, "Shoud be len 1:{:?}", out);
        assert_eq!(out[0], tx3_hash, "Found tx id match{:?}", out);
        service.stop().await.unwrap().await.unwrap();
    }

    #[tokio::test]
    async fn test_find() {
        let config = Config::default();
        let db = Box::new(DummyDb::filled());
        let (_bs, br) = broadcast::channel(10);

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;
        let tx3_hash = *TX_ID3;

        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let service = Service::new(db, config).unwrap();
        service.start(br).await;

        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Insert {
                txs: vec![tx1, tx2],
                response,
            })
            .await;
        let out = receiver.await.unwrap();

        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        let (response, receiver) = oneshot::channel();
        let _ = service
            .sender()
            .send(TxPoolMpsc::Find {
                ids: vec![tx1_hash, tx3_hash],
                response,
            })
            .await;
        let out = receiver.await.unwrap();
        assert_eq!(out.len(), 2, "Shoud be len 2:{:?}", out);
        assert!(out[0].is_some(), "Tx1 should be some:{:?}", out);
        let id = out[0].as_ref().unwrap().id();
        assert_eq!(id, tx1_hash, "Found tx id match{:?}", out);
        assert!(out[1].is_none(), "Tx3 should not be found:{:?}", out);
        service.stop().await.unwrap().await.unwrap();
    }

    /*
    #[tokio::test]
    async fn simple_insert_removal_subscription() {
        let config = Config::default();
        let db = Box::new(DummyDb::filled());

        struct Subs {
            pub new_tx: RwLock<Vec<ArcTx>>,
            pub rem_tx: RwLock<Vec<ArcTx>>,
        }

        #[async_trait]
        impl Subscriber for Subs {
            async fn inserted(&self, tx: ArcTx) {
                self.new_tx.write().await.push(tx);
            }

            async fn inserted_on_block_revert(&self, _tx: ArcTx) {}

            async fn removed(&self, tx: ArcTx, _error: &Error) {
                self.rem_tx.write().await.push(tx);
            }
        }

        let sub = Arc::new(Subs {
            new_tx: RwLock::new(Vec::new()),
            rem_tx: RwLock::new(Vec::new()),
        });

        let tx1_hash = *TX_ID1;
        let tx2_hash = *TX_ID2;

        let tx1 = Arc::new(DummyDb::dummy_tx(tx1_hash));
        let tx2 = Arc::new(DummyDb::dummy_tx(tx2_hash));

        let service = Service::new(db, config).unwrap();
        service.subscribe(sub.clone()).await;
        let out = service.insert(vec![tx1, tx2]).await;
        assert!(out[0].is_ok(), "Tx1 should be OK, got err:{:?}", out);
        assert!(out[1].is_ok(), "Tx2 should be OK, got err:{:?}", out);
        {
            let added = sub.new_tx.read().await;
            assert_eq!(added.len(), 2, "Sub should contains two new tx");
            assert_eq!(added[0].id(), tx1_hash, "First added should be tx1");
            assert_eq!(added[1].id(), tx2_hash, "First added should be tx2");
        }
        service.remove(&[tx1_hash]).await;
        {
            // removing tx1 removed tx2, bcs it is dependent.
            let removed = sub.rem_tx.read().await;
            assert_eq!(removed.len(), 2, "Sub should contains two removed tx");
            assert_eq!(removed[0].id(), tx1_hash, "First removed should be tx1");
            assert_eq!(removed[1].id(), tx2_hash, "Second removed should be tx2");
        }
    }*/
}
