use crate::Config;
use anyhow::Result;
use ethers_providers::{
    Http,
    Middleware,
    Provider,
    ProviderError,
};
use fuel_core_interfaces::relayer::RelayerDb;
use std::ops::Deref;
use tokio::sync::watch;

type Synced = watch::Receiver<bool>;
type Database = Box<dyn RelayerDb>;

pub struct RelayerHandle {
    synced: Synced,
}

pub struct Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    synced: watch::Sender<bool>,
    middleware: P,
    database: Database,
    config: Config,
}

impl<P> Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    fn new(
        synced: watch::Sender<bool>,
        middleware: P,
        database: Database,
        config: Config,
    ) -> Self {
        Self {
            synced,
            middleware,
            database,
            config,
        }
    }
}

impl RelayerHandle {
    pub fn start(database: Database, config: Config) -> Self {
        let middleware = todo!();
        Self::start_inner::<Provider<Http>>(middleware, database, config)
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn start_test<P>(middleware: P, database: Database, config: Config) -> Self
    where
        P: Middleware<Error = ProviderError>,
    {
        Self::start_inner(middleware, database, config)
    }

    fn start_inner<P>(middleware: P, database: Database, config: Config) -> Self
    where
        P: Middleware<Error = ProviderError>,
    {
        let (tx, rx) = watch::channel(false);
        let synced = rx;
        let r = Self {
            synced: synced.clone(),
        };
        run(Relayer::new(tx, middleware, database, config));
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
    P: Middleware<Error = ProviderError> + 'static,
{
    let jh = tokio::task::spawn(async move {
        loop {
            let current_block_height = relayer
                .middleware
                .get_block_number()
                .await
                .unwrap()
                .as_u64();
            if relayer.database.get_finalized_da_height().await
                < current_block_height.saturating_sub(relayer.config.da_finalization)
            {
                // then update da height and download events
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}
