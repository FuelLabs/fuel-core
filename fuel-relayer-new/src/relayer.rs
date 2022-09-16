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
    sync::Arc,
};
use synced::update_synced;
use tokio::sync::watch;

mod state;
mod synced;

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
    eth_node: Arc<P>,
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
            eth_node: Arc::new(eth_node),
            database,
            config,
        }
    }

    async fn download_logs(
        &self,
        eth_sync_gap: &state::EthSyncGap,
    ) -> Result<Vec<Log>, ProviderError> {
        download_logs(
            eth_sync_gap,
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
            let fuel_state = state::FuelLocal::current(
                relayer.database.get_chain_height().await.into(),
            )
            .finalized(
                relayer
                    .database
                    .get_last_committed_finalized_fuel_height()
                    .await
                    .into(),
            );
            let eth_state = state::EthRemote::current(
                relayer.eth_node.get_block_number().await.unwrap().as_u64(),
            )
            .finalization_period(relayer.config.da_finalization)
            .with_local(state::EthLocal::finalized(
                relayer.database.get_finalized_da_height().await,
            ));
            let state = eth_state.with_fuel(fuel_state);

            if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
                // Download events
                let logs = relayer.download_logs(&eth_sync_gap).await.unwrap();

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
                    .set_finalized_da_height(eth_sync_gap.latest())
                    .await;
            }

            if let Some(new_block_height) = state.needs_to_publish_fuel() {
                if let Some(contract) = relayer.config.eth_v2_commit_contract.clone() {
                    let client =
                        crate::abi::fuel::Fuel::new(contract, relayer.eth_node.clone());
                    client
                        .commit_block(
                            new_block_height,
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                        )
                        .call()
                        .await
                        .unwrap();
                }
            }

            update_synced(&relayer.synced, &state);

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}

async fn download_logs<P>(
    eth_sync_gap: &state::EthSyncGap,
    contracts: Vec<H160>,
    eth_node: &P,
) -> Result<Vec<Log>, ProviderError>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let filter = Filter::new()
        .from_block(eth_sync_gap.oldest())
        .to_block(eth_sync_gap.latest())
        .address(ValueOrArray::Array(contracts));

    eth_node.get_logs(&filter).await
}
