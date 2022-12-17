use crate::Config;
use fuel_core_interfaces::block_importer::{
    ImportBlockBroadcast,
    ImportBlockMpsc,
};
use parking_lot::Mutex;
use tokio::{
    sync::{
        broadcast,
        mpsc,
    },
    task::JoinHandle,
};

pub struct Service {
    join: Mutex<Option<JoinHandle<()>>>,
    sender: mpsc::Sender<ImportBlockMpsc>,
    broadcast: broadcast::Sender<ImportBlockBroadcast>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> anyhow::Result<Self> {
        let (sender, _receiver) = mpsc::channel(100);
        let (broadcast, _receiver) = broadcast::channel(100);
        Ok(Self {
            sender,
            broadcast,
            join: Mutex::new(None),
        })
    }

    pub async fn start(&self) {
        let mut join = self.join.lock();
        if join.is_none() {
            *join = Some(tokio::spawn(async {}));
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join = self.join.lock().take();
        if join.is_some() {
            let _ = self.sender.send(ImportBlockMpsc::Stop);
        }
        join
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ImportBlockBroadcast> {
        self.broadcast.subscribe()
    }

    pub fn sender(&self) -> &mpsc::Sender<ImportBlockMpsc> {
        &self.sender
    }
}
