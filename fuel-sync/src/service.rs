use crate::Config;
use fuel_core_interfaces::{
    block_importer::ImportBlockMpsc,
    p2p::{
        BlockGossipData,
        P2pRequestEvent,
    },
};
use parking_lot::Mutex;
use tokio::{
    sync::{
        mpsc,
        oneshot,
    },
    task::JoinHandle,
};

pub enum SyncStatus {
    Stopped,
    InitialSync,
}

pub enum SyncMpsc {
    Status { ret: oneshot::Sender<SyncStatus> },
    Start,
    Stop,
}

pub struct Service {
    join: Mutex<Option<JoinHandle<()>>>,
    sender: mpsc::Sender<SyncMpsc>,
}

impl Service {
    pub async fn new(_config: &Config) -> anyhow::Result<Self> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self {
            sender,
            join: Mutex::new(None),
        })
    }

    pub async fn start(
        &self,
        _p2p_block: mpsc::Receiver<BlockGossipData>,
        _p2p_request: mpsc::Sender<P2pRequestEvent>,
        // TODO: re-introduce this when sync actually depends on the coordinator
        // _bft: mpsc::Sender<BftMpsc>,
        _block_importer: mpsc::Sender<ImportBlockMpsc>,
    ) {
        let mut join = self.join.lock();
        if join.is_none() {
            *join = Some(tokio::spawn(async {}));
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join = self.join.lock().take();
        if join.is_some() {
            let _ = self.sender.send(SyncMpsc::Stop);
        }
        join
    }

    pub fn sender(&self) -> &mpsc::Sender<SyncMpsc> {
        &self.sender
    }
}
