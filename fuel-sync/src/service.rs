use crate::Config;
use fuel_core_interfaces::{
    bft::BFTMpsc,
    block_importer::ImportBlockMpsc,
    // p2p::{BlockBroadcast, P2PMpsc},
    // relayer::RelayerEvent,
    sync::SyncMpsc,
};
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<SyncMpsc>,
}

impl Service {
    pub async fn new(_config: &Config) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self { sender, join: None })
    }

    pub async fn start(
        &mut self,
        _p2p_block: (),   // broadcast::Receiver<BlockBroadcast>,
        _p2p_request: (), // mpsc::Sender<P2PMpsc>,
        _relayer: (),     // mpsc::Sender<RelayerEvent>,
        _bft: mpsc::Sender<BFTMpsc>,
        _block_importer: mpsc::Sender<ImportBlockMpsc>,
    ) {
        self.join = Some(tokio::spawn(async {}));
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(SyncMpsc::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }

    pub fn sender(&self) -> &mpsc::Sender<SyncMpsc> {
        &self.sender
    }
}
