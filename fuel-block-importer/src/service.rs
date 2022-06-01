use crate::Config;
use fuel_core_interfaces::block_importer::{ImportBlockBroadcast, ImportBlockMpsc};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub struct Service {
    join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<ImportBlockMpsc>,
    broadcast: broadcast::Sender<ImportBlockBroadcast>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        let (broadcast, _receiver) = broadcast::channel(100);
        Ok(Self {
            sender,
            broadcast,
            join: None,
        })
    }

    pub async fn start(&mut self) {
        self.join = Some(tokio::spawn(async {}));
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(ImportBlockMpsc::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ImportBlockBroadcast> {
        self.broadcast.subscribe()
    }

    pub fn sender(&self) -> &mpsc::Sender<ImportBlockMpsc> {
        &self.sender
    }
}
