//! # Relayer
//! This module handles bridge communications between
//! the fuel node and the data availability layer.

use crate::{
    log::EthEventLog,
    Config,
};
use anyhow::Result;
use async_trait::async_trait;
use ethers_contract::builders::ContractCall;
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
    model::{
        FuelBlockHeader,
        Message,
    },
    relayer::RelayerDb,
};
use std::{
    convert::TryInto,
    ops::Deref,
    sync::{
        atomic::AtomicBool,
        Arc,
    },
};
use synced::update_synced;
use tokio::sync::watch;

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
    state: RelayerState,
}

struct RelayerShutdown {
    join_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    shutdown: Arc<AtomicBool>,
}

#[derive(Default)]
struct RelayerState {
    pending_committed_fuel_height: Option<tokio::task::JoinHandle<()>>,
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
            state: Default::default(),
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

    async fn write_logs(&mut self, logs: Vec<Log>) -> anyhow::Result<()> {
        write_logs(self.database.as_mut(), logs).await
    }

    async fn publish_fuel_block(&mut self, new_block_height: u32) {
        if !self.is_pending_committed_fuel_height() {
            if let Some(contract) = self.config.eth_v2_commit_contract {
                // FIXME: This should probably be a sync write that flushes to disk to
                // avoid sending duplicate transactions to eth.
                match self
                    .database
                    .get_sealed_block(new_block_height.into())
                    .await
                    .map(|b| b.block.header.clone())
                {
                    Some(new_block) => {
                        self.database
                            .set_pending_committed_fuel_height(Some(
                                new_block_height.into(),
                            ))
                            .await;

                        let jh = spawn_pending_committed_block(
                            contract,
                            self.eth_node.clone(),
                            self.config.da_finalization as usize,
                            self.config.pending_eth_interval,
                            new_block,
                        );

                        self.add_pending_committed_fuel_height(jh);
                    }
                    None => tracing::warn!(
                        "Failed to get fuel block for {}",
                        new_block_height
                    ),
                }
            }
        }
    }

    fn is_pending_committed_fuel_height(&mut self) -> bool {
        match self
            .state
            .pending_committed_fuel_height
            .as_ref()
            .map(tokio::task::JoinHandle::is_finished)
        {
            Some(false) => true,
            Some(true) => {
                self.state.pending_committed_fuel_height.take();
                false
            }
            None => false,
        }
    }

    fn add_pending_committed_fuel_height(&mut self, jh: tokio::task::JoinHandle<()>) {
        self.state.pending_committed_fuel_height = Some(jh);
    }

    async fn shutdown_pending_block_task(self) {
        if let Some(jh) = self.state.pending_committed_fuel_height {
            jh.abort();
            let _ = jh.await;
        }
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
        self.config.da_finalization
    }
}

#[async_trait]
impl<P> state::EthLocal for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    async fn finalized(&self) -> u64 {
        self.database.get_finalized_da_height().await
    }
}

#[async_trait]
impl<P> state::FuelRemote for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    async fn pending(&self) -> Option<u32> {
        self.database
            .get_pending_committed_fuel_height()
            .await
            .map(u32::from)
    }
}

#[async_trait]
impl<P> state::FuelLocal for Relayer<P>
where
    P: Middleware<Error = ProviderError>,
{
    async fn current(&self) -> u32 {
        self.database.get_chain_height().await.into()
    }

    async fn finalized(&self) -> u32 {
        self.database
            .get_last_committed_finalized_fuel_height()
            .await
            .into()
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
                let state = state::build(&relayer).await?;

                if let Some(eth_sync_gap) = state.needs_to_sync_eth() {
                    // Download events
                    let logs = relayer.download_logs(&eth_sync_gap).await?;

                    // Turn logs into eth events
                    relayer.write_logs(logs).await?;

                    // Update finalized height in database.
                    relayer
                        .database
                        .set_finalized_da_height(eth_sync_gap.latest())
                        .await;
                }

                if let Some(new_block_height) = state.needs_to_publish_fuel() {
                    relayer.publish_fuel_block(new_block_height).await;
                }

                update_synced(&relayer.synced, &state);

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }

            relayer.shutdown_pending_block_task().await;

            Ok(())
        }
    });
    RelayerShutdown {
        join_handle,
        shutdown,
    }
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

fn spawn_pending_committed_block<P>(
    contract: H160,
    eth_node: Arc<P>,
    confirmations: usize,
    interval: core::time::Duration,
    new_block: FuelBlockHeader,
) -> tokio::task::JoinHandle<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let client = crate::abi::fuel::Fuel::new(contract, eth_node);

    // TODO: Add pr to add to fuel block header.
    // message_root: merkle root of of messages within this block
    // message count within this block

    let mut block = crate::abi::fuel::fuel::BlockHeader::default();

    // minimum eth block height
    let eth_block_number = *new_block.number;

    // fuel block height
    block.height = *new_block.height;
    // prev_root merkle root of all previous blocks
    block.previous_block_root = *new_block.prev_root;

    block.block_number = eth_block_number;

    let txn = client.commit_block(
        eth_block_number,
        Default::default(),
        block,
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    tokio::task::spawn(async move {
        if let Err(e) = await_pending_committed_block(txn, confirmations, interval).await
        {
            tracing::warn!(
                "awaiting a pending committed fuel block task died with: {}",
                e
            );
        }
    })
}

async fn await_pending_committed_block<P>(
    txn: ContractCall<P, ()>,
    confirmations: usize,
    interval: core::time::Duration,
) -> anyhow::Result<()>
where
    P: Middleware<Error = ProviderError> + 'static,
{
    let pending_transaction = txn
        .send()
        .await?
        .confirmations(confirmations)
        .interval(interval);

    pending_transaction
        .await?
        .map(|_| ())
        .ok_or_else(|| anyhow::anyhow!("Failed to get pending transaction"))
}

async fn write_logs(database: &mut dyn RelayerDb, logs: Vec<Log>) -> anyhow::Result<()> {
    let events: Vec<EthEventLog> = logs
        .iter()
        .map(|l| l.try_into())
        .collect::<Result<_, _>>()?;
    for event in events {
        match event {
            EthEventLog::Message(m) => {
                let m: Message = (&m).into();
                // Add messages to database
                database.insert(&m.id(), &m)?;
            }
            EthEventLog::FuelBlockCommitted { height, .. } => {
                // TODO: Check if this is greater then current.
                database
                    .set_last_committed_finalized_fuel_height(height.into())
                    .await;

                database.set_pending_committed_fuel_height(None).await;
            }
            EthEventLog::Ignored => todo!(),
        }
    }
    Ok(())
}
