use crate::{
    log::EthEventLog,
    Config,
};
use anyhow::Result;
use ethers_core::types::{
    Filter,
    Log,
    ValueOrArray,
    H160,
};
use ethers_providers::{
    Http,
    Middleware,
    Provider,
    ProviderError,
};
use fuel_core_interfaces::{
    model::Message,
    relayer::RelayerDb,
};
use std::{
    convert::TryInto,
    ops::Deref,
};
use tokio::sync::watch;

#[cfg(test)]
mod test;

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
    P: Middleware<Error = ProviderError> + 'static,
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

    async fn download_logs(
        &self,
        current_local_block_height: u64,
        latest_finalized_block: u64,
    ) -> Result<Vec<Log>, ProviderError> {
        download_logs(
            current_local_block_height,
            latest_finalized_block,
            self.config.eth_v2_listening_contracts.clone(),
            &self.eth_node,
        )
        .await
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

fn run<P>(mut relayer: Relayer<P>)
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let jh = tokio::task::spawn(async move {
        loop {
            let current_block_height =
                relayer.eth_node.get_block_number().await.unwrap().as_u64();
            let latest_finalized_block =
                current_block_height.saturating_sub(relayer.config.da_finalization);
            let current_local_block_height =
                relayer.database.get_finalized_da_height().await;
            let out_of_sync = current_local_block_height < latest_finalized_block;

            // Update our state for handles.
            relayer.synced.send_if_modified(|last_state| {
                let in_sync = !out_of_sync;
                let changed = *last_state == in_sync;
                *last_state |= in_sync;
                changed
            });

            if out_of_sync {
                // Download events
                let logs = relayer
                    .download_logs(current_local_block_height, latest_finalized_block)
                    .await
                    .unwrap();

                // Turn logs into eth events
                let events: Vec<EthEventLog> =
                    logs.iter().map(|l| l.try_into().unwrap()).collect();
                for event in events {
                    match event {
                        EthEventLog::Message(m) => {
                            let m: Message = (&m).into();
                            // Add messages to database
                            relayer.database.insert(&m.id(), &m).unwrap();
                        }
                        EthEventLog::FuelBlockCommitted { block_root, height } => {
                            // TODO: Check if this is greater then current.
                            relayer
                                .database
                                .set_last_committed_finalized_fuel_height(height.into())
                                .await;
                        }
                        EthEventLog::Ignored => todo!(),
                    }
                }

                // Update finalized height in database.
                relayer
                    .database
                    .set_finalized_da_height(latest_finalized_block)
                    .await;
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}

async fn download_logs<P>(
    current_local_block_height: u64,
    latest_finalized_block: u64,
    contracts: Vec<H160>,
    eth_node: &P,
) -> Result<Vec<Log>, ProviderError>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let filter = Filter::new()
        .from_block(current_local_block_height)
        .to_block(latest_finalized_block)
        .address(ValueOrArray::Array(contracts));

    eth_node.get_logs(&filter).await
}
