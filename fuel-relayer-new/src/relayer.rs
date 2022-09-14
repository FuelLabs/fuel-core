use anyhow::Result;
use ethers_providers::Http;
use ethers_providers::Middleware;
use ethers_providers::Provider;
use ethers_providers::ProviderError;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::watch;

type Synced = watch::Receiver<bool>;

pub struct RelayerHandle {
    synced: Synced,
}

pub struct Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    synced: watch::Sender<bool>,
    middleware: P,
}

impl<P> Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    fn new(synced: watch::Sender<bool>, middleware: P) -> Self {
        Self { synced, middleware }
    }
}

impl RelayerHandle {
    pub fn start() -> Self {
        let middleware = todo!();
        Self::start_inner::<Provider<Http>>(middleware)
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn start_test<P>(middleware: P) -> Self
    where
        P: Middleware<Error = ProviderError>,
    {
        use std::sync::Arc;

        Self::start_inner(middleware)
    }

    fn start_inner<P>(middleware: P) -> Self
    where
        P: Middleware<Error = ProviderError>,
    {
        let (tx, rx) = watch::channel(false);
        let synced = rx;
        let r = Self {
            synced: synced.clone(),
        };
        run(Relayer::new(tx, middleware));
        r
    }

    pub async fn await_synced(&self) -> Result<()> {
        let mut rx = self.synced.clone();
        if !rx.borrow_and_update().deref() {
            rx.changed().await?;
        }
        Ok(())
    }
}

fn run<P>(relayer: Relayer<P>)
where
    P: Middleware<Error = ProviderError>,
{
    let jh = tokio::task::spawn(async move {
        loop {
            if relayer.synced.send(true).is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}
