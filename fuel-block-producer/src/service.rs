use crate::Config;
use fuel_core_interfaces::{block_producer::BlockProducerMpsc, txpool::TxPoolMpsc};
use parking_lot::Mutex;
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    join: Mutex<Option<JoinHandle<()>>>,
    sender: mpsc::Sender<BlockProducerMpsc>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self {
            sender,
            join: Mutex::new(None),
        })
    }

    pub async fn start(&self, _txpool: mpsc::Sender<TxPoolMpsc>) {
        let mut join = self.join.lock();
        if join.is_none() {
            *join = Some(tokio::spawn(async {}));
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join = self.join.lock().take();
        if join.is_some() {
            let _ = self.sender.send(BlockProducerMpsc::Stop);
        }
        join
    }

    pub fn sender(&self) -> &mpsc::Sender<BlockProducerMpsc> {
        &self.sender
    }
}
