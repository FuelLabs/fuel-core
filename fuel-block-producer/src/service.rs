use crate::Config;
use fuel_core_interfaces::block_producer::BlockProducerMpsc;
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<BlockProducerMpsc>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self { sender, join: None })
    }

    pub async fn start(&mut self, _txpool: ()) {
        self.join = Some(tokio::spawn(async {}));
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(BlockProducerMpsc::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }

    pub fn sender(&self) -> &mpsc::Sender<BlockProducerMpsc> {
        &self.sender
    }
}
