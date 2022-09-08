use crate::{
    block_producer::Task,
    db::BlockProducerDatabase,
    ports::{
        Relayer,
        TxPool,
    },
    Config,
};
use fuel_core_interfaces::{
    block_producer::{
        BlockProducerBroadcast,
        BlockProducerMpsc,
    },
    executor::Executor,
    txpool,
};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::{
    sync::{
        broadcast,
        mpsc,
        Mutex,
    },
    task::JoinHandle,
};
use tracing::{
    info,
    warn,
};

/// Primary entrypoint for the block producer.
/// Manages channels and tasks related to block production (i.e. supervisor)
/// External dependencies are interfaced for maximal decoupling, modularity and testability.
pub struct Service {
    join: Mutex<Option<JoinHandle<mpsc::Receiver<BlockProducerMpsc>>>>,
    sender: mpsc::Sender<BlockProducerMpsc>,
    broadcast: broadcast::Sender<BlockProducerBroadcast>,
    config: Config,
    receiver: Arc<Mutex<Option<mpsc::Receiver<BlockProducerMpsc>>>>,
}

impl Service {
    pub async fn new(config: &Config, _db: ()) -> Result<Self, anyhow::Error> {
        let (sender, receiver) = mpsc::channel(100);
        let (broadcast, _receiver) = broadcast::channel(100);
        Ok(Self {
            sender,
            join: Mutex::new(None),
            broadcast,
            config: config.clone(),
            receiver: Arc::new(Mutex::new(Some(receiver))),
        })
    }

    pub async fn start(
        &self,
        txpool: Box<dyn TxPool>,
        relayer: Box<dyn Relayer>,
        executor: Box<dyn Executor>,
        db: Box<dyn BlockProducerDatabase>,
    ) -> bool {
        let mut join = self.join.lock().await;
        if join.is_none() {
            if let Some(receiver) = self.receiver.lock().await.take() {
                let task = Task {
                    receiver,
                    config: self.config.clone(),
                    db,
                    relayer,
                    txpool,
                    executor,
                };
                *join = Some(tokio::spawn(task.spawn()));
                return true
            } else {
                warn!("Starting block producer that is stopping");
            }
        } else {
            warn!("Service block producer is already started")
        }
        false
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        info!("stopping block producer service...");
        let join = self.join.lock().await.take();
        if let Some(join_handle) = join {
            let _ = self.sender.send(BlockProducerMpsc::Stop);
            let receiver = self.receiver.clone();
            Some(tokio::spawn(async move {
                let ret = join_handle.await;
                *receiver.lock().await = ret.ok();
            }))
        } else {
            None
        }
    }

    pub fn sender(&self) -> &mpsc::Sender<BlockProducerMpsc> {
        &self.sender
    }

    pub fn subscribe_ch(&self) -> broadcast::Receiver<BlockProducerBroadcast> {
        self.broadcast.subscribe()
    }
}
