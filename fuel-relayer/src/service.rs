use crate::{Config, Relayer};
use anyhow::{anyhow, Error};
use ethers_providers::{Http, Middleware, Provider, ProviderError, Ws};
use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    relayer::{self, RelayerDb, RelayerRequest},
};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc, Mutex},
    task::JoinHandle,
};
use url::Url;

const PROVIDER_INTERVAL: u64 = 1000;

pub struct ServiceBuilder {
    sender: relayer::Sender,
    receiver: mpsc::Receiver<RelayerRequest>,
    private_key: Option<Vec<u8>>,
    db: Option<Box<dyn RelayerDb>>,
    import_block_events: Option<broadcast::Receiver<ImportBlockBroadcast>>,
    config: Config,
}

impl Default for ServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBuilder {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(100);
        Self {
            sender: relayer::Sender::new(sender),
            receiver,
            private_key: None,
            db: None,
            import_block_events: None,
            config: Default::default(),
        }
    }
    pub fn sender(&mut self) -> &relayer::Sender {
        &self.sender
    }

    pub fn private_key(&mut self, private_key: Vec<u8>) -> &mut Self {
        self.private_key = Some(private_key);
        self
    }
    pub fn db(&mut self, db: Box<dyn RelayerDb>) -> &mut Self {
        self.db = Some(db);
        self
    }

    pub fn import_block_event(
        &mut self,
        import_block_event: broadcast::Receiver<ImportBlockBroadcast>,
    ) -> &mut Self {
        self.import_block_events = Some(import_block_event);
        self
    }

    pub fn config(&mut self, config: Config) -> &mut Self {
        self.config = config;
        self
    }

    pub fn build(self) -> anyhow::Result<Service> {
        if self.private_key.is_none() || self.db.is_none() || self.import_block_events.is_none() {
            return Err(anyhow!("One of context items are not set"));
        }
        let service = Service::new(
            self.sender,
            Context {
                receiver: self.receiver,
                private_key: self.private_key.unwrap(),
                db: self.db.unwrap(),
                new_block_event: self.import_block_events.unwrap(),
                config: self.config,
            },
        )?;
        Ok(service)
    }
}

pub struct Service {
    sender: relayer::Sender,
    join: Mutex<Option<JoinHandle<Context>>>,
    context: Arc<Mutex<Option<Context>>>,
}

pub struct Context {
    /// Request channel.
    pub receiver: mpsc::Receiver<RelayerRequest>,
    /// Private.
    pub private_key: Vec<u8>,
    /// Db connector to apply stake and token deposit.
    pub db: Box<dyn RelayerDb>,
    /// Notification of new block event.
    pub new_block_event: broadcast::Receiver<ImportBlockBroadcast>,
    /// Relayer Configuration.
    pub config: Config,
}

impl Service {
    pub(crate) fn new(sender: relayer::Sender, context: Context) -> anyhow::Result<Self> {
        Ok(Self {
            sender,
            join: Mutex::new(None),
            context: Arc::new(Mutex::new(Some(context))),
        })
    }

    /// create provider that we use for communication with ethereum.
    pub(crate) async fn provider_ws(uri: &str) -> Result<Provider<Ws>, Error> {
        let ws = Ws::connect(uri).await?;
        let provider =
            Provider::new(ws).interval(std::time::Duration::from_millis(PROVIDER_INTERVAL));
        Ok(provider)
    }

    pub(crate) fn provider_http(uri: &str) -> Result<Provider<Http>, Error> {
        let url = Url::parse(uri).unwrap();
        let ws = Http::new(url);
        let provider =
            Provider::new(ws).interval(std::time::Duration::from_millis(PROVIDER_INTERVAL));
        Ok(provider)
    }

    // internal function to simplify code for different prividers.
    async fn run<P>(
        &self,
        join: &mut Option<JoinHandle<Context>>,
        context: Context,
        provider: anyhow::Result<P>,
    ) -> anyhow::Result<()>
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let provider = match provider {
            Ok(provider) => provider,
            Err(e) => {
                *self.context.lock().await = Some(context);
                return Err(e);
            }
        };
        let relayer = Relayer::new(context).await;
        *join = Some(tokio::spawn(async {
            relayer.run(Arc::new(provider)).await
        }));
        Ok(())
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let mut join = self.join.lock().await;
        if join.is_none() {
            if let Some(context) = self.context.lock().await.take() {
                let uri = context.config.eth_client.clone().unwrap_or_default();
                if uri.starts_with("http") {
                    self.run(&mut *join, context, Self::provider_http(&uri))
                        .await
                } else if uri.starts_with("ws") {
                    self.run(&mut *join, context, Self::provider_ws(&uri).await)
                        .await
                } else {
                    *self.context.lock().await = Some(context);
                    Err(anyhow!("Relayer client uri is not http or ws"))
                }
            } else {
                Err(anyhow!("Starting FuelRelayer service that is stopping"))
            }
        } else {
            Err(anyhow!("Service FuelRelayer is already started"))
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let mut join = self.join.lock().await;
        let join_handle = join.take();
        if let Some(join_handle) = join_handle {
            let _ = self.sender.send(RelayerRequest::Stop).await;
            let context = self.context.clone();
            Some(tokio::spawn(async move {
                let ret = join_handle.await;
                *context.lock().await = ret.ok();
            }))
        } else {
            None
        }
    }

    pub fn sender(&self) -> &relayer::Sender {
        &self.sender
    }
}
