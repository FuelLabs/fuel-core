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
    eth_node: P,
    database: Database,
    config: Config,
}

impl<P> Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    fn new(
        synced: watch::Sender<bool>,
        eth_node: P,
        database: Database,
        config: Config,
    ) -> Self {
        Self {
            synced,
            eth_node,
            database,
            config,
        }
    }
}

impl RelayerHandle {
    pub fn start(database: Database, config: Config) -> Self {
        let eth_node = todo!();
        Self::start_inner::<Provider<Http>>(eth_node, database, config)
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn start_test<P>(eth_node: P, database: Database, config: Config) -> Self
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        Self::start_inner(eth_node, database, config)
    }

    fn start_inner<P>(eth_node: P, database: Database, config: Config) -> Self
    where
        P: Middleware<Error = ProviderError> + 'static,
    {
        let (tx, rx) = watch::channel(false);
        let synced = rx;
        let r = Self {
            synced: synced.clone(),
        };
        run(Relayer::new(tx, eth_node, database, config));
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
            let current_block_height =
                relayer.eth_node.get_block_number().await.unwrap().as_u64();
            let latest_finalized_block =
                current_block_height.saturating_sub(relayer.config.da_finalization);
            let out_of_sync =
                relayer.database.get_finalized_da_height().await < latest_finalized_block;

            // Update our state for handles.
            let sent = relayer.synced.send_if_modified(|last_state| {
                let in_sync = !out_of_sync;
                let changed = *last_state == in_sync;
                *last_state |= in_sync;
                changed
            });
            sent;

            if out_of_sync {
                relayer
                    .database
                    .set_finalized_da_height(latest_finalized_block)
                    .await;
                // then update da height and download events
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}
