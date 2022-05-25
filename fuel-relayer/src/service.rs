use crate::{Config, Relayer};
use anyhow::Error;
use ethers_providers::{Http, Middleware, Provider, ProviderError, Ws};
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    relayer::{RelayerDb, RelayerEvent},
};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use url::Url;

const PROVIDER_INTERVAL: u64 = 1000;

pub struct Service {
    stop_join: Option<JoinHandle<()>>,
    sender: mpsc::Sender<RelayerEvent>,
}

impl Service {
    pub async fn new<P>(
        config: &Config,
        private_key: &[u8],
        db: Box<dyn RelayerDb>,
        new_block_event: broadcast::Receiver<NewBlockEvent>,
        provider: Arc<P>,
    ) -> Result<Self, anyhow::Error>
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let (sender, receiver) = mpsc::channel(100);
        let relayer =
            Relayer::new(config.clone(), private_key, db, receiver, new_block_event).await;

        let stop_join = Some(tokio::spawn(Relayer::run(relayer, provider)));
        Ok(Self { sender, stop_join })
    }

    pub async fn stop(&mut self) {
        let _ = self.sender.send(RelayerEvent::Stop);
        if let Some(join) = self.stop_join.take() {
            let _ = join.await;
        }
    }

    /// create provider that we use for communication with ethereum.
    pub async fn provider(uri: &str) -> Result<Provider<Ws>, Error> {
        let ws = Ws::connect(uri).await?;
        let provider =
            Provider::new(ws).interval(std::time::Duration::from_millis(PROVIDER_INTERVAL));
        Ok(provider)
    }

    pub fn provider_http(uri: &str) -> Result<Provider<Http>, Error> {
        let url = Url::parse(uri).unwrap();
        let ws = Http::new(url);
        let provider =
            Provider::new(ws).interval(std::time::Duration::from_millis(PROVIDER_INTERVAL));
        Ok(provider)
    }

    pub fn sender(&self) -> &mpsc::Sender<RelayerEvent> {
        &self.sender
    }
}
