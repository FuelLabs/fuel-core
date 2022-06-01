use crate::Config;
use fuel_core_interfaces::{
    block_importer::{ImportBlockMpsc, ImportBlockBroadcast}, block_producer::BlockProducerMpsc, relayer::RelayerEvent,
};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub struct Service {
    join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<RelayerEvent>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self { sender, join: None })
    }

    pub async fn start(
        &mut self,
        _relayer: (),
        _block_producer: mpsc::Sender<BlockProducerMpsc>,
        _block_importer_sender: mpsc::Sender<ImportBlockMpsc>,
        _block_importer_broadcast: broadcast::Receiver<ImportBlockBroadcast>,
    ) {
        self.join = Some(tokio::spawn(async {}));
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(RelayerEvent::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }

    pub fn sender(&self) -> &mpsc::Sender<RelayerEvent> {
        &self.sender
    }
}
