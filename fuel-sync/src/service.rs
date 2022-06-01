use crate::Config;
use fuel_core_interfaces::relayer::RelayerEvent;
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<RelayerEvent>,
}

impl Service {
    pub async fn new(_config: &Config) -> Result<Self, anyhow::Error> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self { sender, join: None })
    }

    pub async fn start(&mut self) {
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
