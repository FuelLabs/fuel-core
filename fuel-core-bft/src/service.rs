use crate::Config;
use fuel_core_interfaces::{
    bft::BftMpsc,
    block_importer::{ImportBlockBroadcast, ImportBlockMpsc},
    block_producer::BlockProducerMpsc,
    p2p::P2pRequestEvent,
    relayer,
};
use parking_lot::Mutex;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub struct Service {
    join: Mutex<Option<JoinHandle<()>>>,
    sender: mpsc::Sender<BftMpsc>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> anyhow::Result<Self> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self {
            sender,
            join: Mutex::new(None),
        })
    }

    pub async fn start(
        &self,
        _relayer: relayer::Sender,
        _p2p_consensus: mpsc::Sender<P2pRequestEvent>,
        _block_producer: mpsc::Sender<BlockProducerMpsc>,
        _block_importer_sender: mpsc::Sender<ImportBlockMpsc>,
        _block_importer_broadcast: broadcast::Receiver<ImportBlockBroadcast>,
    ) {
        let mut join = self.join.lock();
        if join.is_none() {
            *join = Some(tokio::spawn(async {}));
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join = self.join.lock().take();
        if join.is_some() {
            let _ = self.sender.send(BftMpsc::Stop);
        }
        join
    }

    pub fn sender(&self) -> &mpsc::Sender<BftMpsc> {
        &self.sender
    }
}
