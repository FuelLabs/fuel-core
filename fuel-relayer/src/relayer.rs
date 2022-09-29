//! # Relayer
//! This module handles bridge communications between
//! the fuel node and the data availability layer.

use crate::{
    log::EthEventLog,
    Config,
};
use anyhow::Result;
use async_trait::async_trait;
use core::time::Duration;
use ethers_core::types::{
    Filter,
    Log,
    ValueOrArray,
    H160,
    H256,
};
use ethers_providers::{
    Http,
    Middleware,
    Provider,
    ProviderError,
    SyncingStatus,
};
use fuel_core_interfaces::{
    common::{
        crypto,
        prelude::{
            Output,
            SizedBytes,
        },
    },
    db::Messages,
    model::{
        FuelBlock,
        Message,
        SealedFuelBlock,
    },
    relayer::RelayerDb,
};
use std::{
    convert::TryInto,
    io::Read,
    ops::Deref,
    sync::{
        atomic::AtomicBool,
        Arc,
    },
};
use synced::update_synced;
use tokio::sync::watch;

use self::{
    get_logs::*,
    publish::*,
    run::RelayerData,
};

mod get_logs;
mod publish;
mod run;
mod state;
mod synced;

#[cfg(test)]
mod test;

type Synced = watch::Receiver<bool>;
type Database = Box<dyn RelayerDb>;

/// Handle for interacting with the [`Relayer`].
pub struct RelayerHandle {
    synced: Synced,
    shutdown: RelayerShutdown,
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

struct RelayerShutdown {
    join_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    shutdown: Arc<AtomicBool>,
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
}

#[async_trait]
impl<P> RelayerData for Relayer<P>
where
    P: Middleware<Error = ProviderError> + 'static,
{
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

    async fn write_logs(&mut self, logs: Vec<Log>) -> anyhow::Result<()> {
        write_logs(self.database.as_mut(), logs).await
    }
    async fn publish_fuel_block(&mut self) -> anyhow::Result<()> {
        match self.config.eth_v2_commit_contract {
            Some(contract) => {
                publish_fuel_block(
                    self.database.as_mut(),
                    self.eth_node.clone(),
                    contract,
                )
                .await
            }
            None => Ok(()),
        }
    }

    async fn set_finalized_da_height(
        &self,
        height: fuel_core_interfaces::model::DaBlockHeight,
    ) {
        self.database.set_finalized_da_height(height).await
    }

    fn update_synced(&self, state: &state::SyncState) {
        update_synced(&self.synced, state)
    }

    async fn wait_if_eth_syncing(&self) -> anyhow::Result<()> {
        let mut count = 0;
        while matches!(
            self.eth_node.syncing().await?,
            SyncingStatus::IsSyncing { .. }
        ) {
            count += 1;
            if count > 12 {
                tracing::warn!(
                    "relayer has been waiting longer then a minute for the eth node to sync"
                )
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        Ok(())
    }
}

impl RelayerHandle {
    /// Start a http [`Relayer`] running and return the handle to it.
    pub fn start(database: Database, config: Config) -> anyhow::Result<Self> {
        let url = config.eth_client.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Tried to start Relayer without setting an eth_client in the config"
            )
        })?;
        // TODO: Does this handle https?
        let http = Http::new(url);
        let eth_node = Provider::new(http);
        Ok(Self::start_inner::<Provider<Http>>(
            eth_node, database, config,
        ))
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Start a test relayer.
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
        let shutdown = run(Relayer::new(tx, eth_node, database, config));
        Self { synced, shutdown }
    }

    /// Wait for the [`Relayer`] to be in sync with
    /// the data availability layer.
    ///
    /// Yields until the relayer reaches a point where it
    /// considered up to date. Note that there's no guarantee
    /// the relayer will ever catch up to the da layer and
    /// may fall behind immediately after this future completes.
    ///
    /// The only guarantee is that if this future completes then
    /// the relayer did reach consistency with the da layer for
    /// some period of time.
    pub async fn await_synced(&self) -> Result<()> {
        let mut rx = self.synced.clone();
        if !rx.borrow_and_update().deref() {
            rx.changed().await?;
        }
        Ok(())
    }

    /// Check if the [`Relayer`] is still running.
    pub fn is_running(&self) -> bool {
        !self.shutdown.join_handle.is_finished()
    }

    /// Gracefully shutdown the [`Relayer`].
    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.shutdown
            .shutdown
            .store(true, core::sync::atomic::Ordering::Relaxed);
        self.shutdown.join_handle.await?
    }
}

#[async_trait]
impl<P> state::EthRemote for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    async fn current(&self) -> anyhow::Result<u64> {
        Ok(self.eth_node.get_block_number().await?.as_u64())
    }

    fn finalization_period(&self) -> u64 {
        *self.config.da_finalization
    }
}

#[async_trait]
impl<P> state::EthLocal for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    async fn finalized(&self) -> u64 {
        *self.database.get_finalized_da_height().await
    }
}

#[async_trait]
impl<P> state::FuelLocal for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    fn message_time_window(&self) -> std::time::Duration {
        self.config.fuel_publish_window
    }

    fn min_messages_to_force_publish(&self) -> usize {
        self.config.fuel_min_force_publish
    }

    async fn last_sent_time(&self) -> Option<Duration> {
        match self.database.get_last_published_fuel_height().await {
            Some(height) => self
                .database
                .get_sealed_block(height)
                .await
                .and_then(|block| get_unix_time_from_block(block.as_ref())),
            None => None,
        }
    }

    async fn latest_block_time(&self) -> Option<Duration> {
        let height = self.database.get_chain_height().await;
        self.database
            .get_sealed_block(height)
            .await
            .and_then(|block| get_unix_time_from_block(block.as_ref()))
    }

    async fn num_unpublished_messages(&self) -> usize {
        let mut height = self
            .database
            .get_last_published_fuel_height()
            .await
            .unwrap_or_default();

        let mut count = 0;

        while let Some(block) = self.database.get_sealed_block(height).await {
            count += block
                .block
                .transactions
                .iter()
                .map(|t| {
                    t.outputs()
                        .iter()
                        .filter(|o| matches!(o, Output::Message { .. }))
                        .count()
                })
                .sum::<usize>();
            height = match height.checked_add(1) {
                Some(h) => h.into(),
                None => return count,
            }
        }
        count
    }
}

fn run<P>(mut relayer: Relayer<P>) -> RelayerShutdown
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let shutdown = Arc::new(AtomicBool::new(false));
    let join_handle = tokio::task::spawn({
        let shutdown = shutdown.clone();
        async move {
            while !shutdown.load(core::sync::atomic::Ordering::Relaxed) {
                run::run(&mut relayer).await?;

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Ok(())
        }
    });
    RelayerShutdown {
        join_handle,
        shutdown,
    }
}

fn get_unix_time_from_block(block: &SealedFuelBlock) -> Option<Duration> {
    let t: std::time::SystemTime = block.block.header.time.into();
    t.duration_since(std::time::UNIX_EPOCH).ok()
}
