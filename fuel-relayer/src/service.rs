use crate::Config;
use crate::Relayer;
use fuel_core_interfaces::relayer::{RelayerDB, RelayerEvent};
use tokio::{sync::mpsc, task::JoinHandle};

pub struct Service {
    stop_join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<RelayerEvent>,
}

impl Service {
    pub async fn new(
        config: &Config,
        db: Box<dyn RelayerDB>,
        best_block: u64,
    ) -> Result<Self, anyhow::Error> {
        let (sender, receiver) = mpsc::channel(100);
        let relayer = Relayer::new(config.clone(), db, receiver);
        let provider = Relayer::provider(config.eth_client()).await?;
        let stop_join = Some(tokio::spawn(Relayer::run(relayer, provider, best_block)));
        Ok(Self { sender, stop_join })
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(RelayerEvent::Stop);
        if let Some(join) = self.stop_join.take() {
            let _ = join.await;
        }
    }

    pub fn sender(&self) -> &mpsc::Sender<RelayerEvent> {
        &self.sender
    }
}
