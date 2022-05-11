use std::sync::Arc;

use crate::Config;
use crate::Relayer;
use ethers_providers::{Middleware, ProviderError};
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    relayer::{RelayerDb, RelayerEvent},
};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub struct Service {
    stop_join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<RelayerEvent>,
}

impl Service {
    pub async fn new<P>(
        config: &Config,
        db: Box<dyn RelayerDb>,
        new_block_event: broadcast::Receiver<NewBlockEvent>,
        provider: Arc<P>,
    ) -> Result<Self, anyhow::Error>
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let (sender, receiver) = mpsc::channel(100);
        let relayer = Relayer::new(config.clone(), db, receiver, new_block_event).await;

        let stop_join = Some(tokio::spawn(Relayer::run(relayer, provider)));
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
