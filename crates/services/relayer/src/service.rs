//! This module handles bridge communications between the fuel node and the data availability layer.

use crate::{
    Config,
    log::EthEventLog,
    ports::RelayerDb,
    service::state::EthLocal,
};
use async_trait::async_trait;
use core::time::Duration;
use ethers_core::types::{
    Filter,
    Log,
    SyncingStatus,
    ValueOrArray,
};
use ethers_providers::{
    Http,
    Middleware,
    Provider,
    ProviderError,
    Quorum,
    QuorumProvider,
    WeightedProvider,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::Message,
};
use futures::StreamExt;
use std::convert::TryInto;
use tokio::sync::watch;

use self::{
    get_logs::*,
    run::RelayerData,
};

mod get_logs;
mod run;
mod state;
mod syncing;

#[cfg(test)]
mod test;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// The relayer is partially synced with the DA layer. The variant contains the current
    /// DA block height that the relayer synced to.
    PartiallySynced(DaBlockHeight),
    /// The relayer is fully synced with the DA layer.
    Synced(DaBlockHeight),
}

impl SyncState {
    /// Get the current DA block height that the relayer synced to.
    pub fn da_block_height(&self) -> DaBlockHeight {
        match self {
            Self::PartiallySynced(height) | Self::Synced(height) => *height,
        }
    }

    /// Returns `true` if the relayer is fully synced with the DA layer.
    pub fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }
}

type Synced = watch::Receiver<SyncState>;
type NotifySynced = watch::Sender<SyncState>;

/// The alias of runnable relayer service.
pub type Service<D> = CustomizableService<Provider<QuorumProvider<Http>>, D>;
type CustomizableService<P, D> = ServiceRunner<NotInitializedTask<P, D>>;

/// The shared state of the relayer task.
#[derive(Clone)]
pub struct SharedState {
    /// Receives signals when the relayer reaches consistency with the DA layer.
    synced: Synced,
}

/// Not initialized version of the [`Task`].
pub struct NotInitializedTask<P, D> {
    /// Sends signals when the relayer reaches consistency with the DA layer.
    synced: NotifySynced,
    /// The node that communicates with Ethereum.
    eth_node: P,
    /// The fuel database.
    database: D,
    /// Configuration settings.
    config: Config,
    /// Retry on error
    retry_on_error: bool,
}

/// The actual relayer background task that syncs with the DA layer.
pub struct Task<P, D> {
    /// Sends signals when the relayer reaches consistency with the DA layer.
    synced: NotifySynced,
    /// The node that communicates with Ethereum.
    eth_node: P,
    /// The fuel database.
    database: D,
    /// Configuration settings.
    config: Config,
    /// The watcher used to track the state of the service. If the service stops,
    /// the task will stop synchronization.
    shutdown: StateWatcher,
    /// Retry on error
    retry_on_error: bool,
}

impl<P, D> NotInitializedTask<P, D>
where
    D: RelayerDb + 'static,
{
    /// Create a new relayer task.
    fn new(eth_node: P, database: D, config: Config, retry_on_error: bool) -> Self {
        let da_block_height = database.get_finalized_da_height().unwrap_or_else(|| {
            let height_before_deployed = config.da_deploy_height.0.saturating_sub(1);
            height_before_deployed.into()
        });

        let (synced, _) = watch::channel(SyncState::PartiallySynced(da_block_height));

        Self {
            synced,
            eth_node,
            database,
            config,
            retry_on_error,
        }
    }
}

impl<P, D> RelayerData for Task<P, D>
where
    P: Middleware<Error = ProviderError> + 'static,
    D: RelayerDb + 'static,
{
    async fn wait_if_eth_syncing(&self) -> anyhow::Result<()> {
        let mut shutdown = self.shutdown.clone();
        tokio::select! {
            biased;
            _ = shutdown.while_started() => {
                Err(anyhow::anyhow!("The relayer got a stop signal"))
            },
            result = syncing::wait_if_eth_syncing(
                &self.eth_node,
                self.config.syncing_call_frequency,
                self.config.syncing_log_frequency,
            ) => {
                result
            }
        }
    }

    async fn download_logs(
        &mut self,
        eth_sync_gap: &state::EthSyncGap,
    ) -> anyhow::Result<()> {
        let logs = download_logs(
            eth_sync_gap,
            self.config.eth_v2_listening_contracts.clone(),
            &self.eth_node,
            self.config.log_page_size,
        );
        let logs = logs.take_until(self.shutdown.while_started());

        write_logs(&mut self.database, logs).await
    }

    fn update_synced(&self, state: &state::EthState) {
        self.synced.send_if_modified(|last_state| {
            let new_sync = state.sync_state();
            if new_sync != *last_state {
                *last_state = new_sync;
                true
            } else {
                false
            }
        });
    }

    fn storage_da_block_height(&self) -> Option<u64> {
        self.database
            .get_finalized_da_height()
            .map(|height| height.into())
    }
}

#[async_trait]
impl<P, D> RunnableService for NotInitializedTask<P, D>
where
    P: Middleware<Error = ProviderError> + 'static,
    D: RelayerDb + 'static,
{
    const NAME: &'static str = "Relayer";

    type SharedData = SharedState;
    type Task = Task<P, D>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        let synced = self.synced.subscribe();

        SharedState { synced }
    }

    async fn into_task(
        mut self,
        watcher: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let shutdown = watcher.clone();
        let NotInitializedTask {
            synced,
            eth_node,
            database,
            config,
            retry_on_error,
        } = self;
        let task = Task {
            synced,
            eth_node,
            database,
            config,
            shutdown,
            retry_on_error,
        };

        Ok(task)
    }
}

impl<P, D> RunnableTask for Task<P, D>
where
    P: Middleware<Error = ProviderError> + 'static,
    D: RelayerDb + 'static,
{
    async fn run(&mut self, _: &mut StateWatcher) -> TaskNextAction {
        let now = tokio::time::Instant::now();

        let result = run::run(self).await;

        if self.shutdown.borrow_and_update().started()
            && (result.is_err() | self.synced.borrow().is_synced())
        {
            // Sleep the loop so the da node is not spammed.
            tokio::time::sleep(
                self.config
                    .sync_minimum_duration
                    .saturating_sub(now.elapsed()),
            )
            .await;
        }

        match result {
            Err(err) => {
                if !self.retry_on_error {
                    tracing::error!("Exiting due to Error in relayer task: {}", err);
                    TaskNextAction::Stop
                } else {
                    TaskNextAction::ErrorContinue(err)
                }
            }
            _ => TaskNextAction::Continue,
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
        Ok(())
    }
}

impl SharedState {
    /// Wait for the `Task` to be in sync with
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
    pub async fn await_synced(&self) -> anyhow::Result<()> {
        let mut rx = self.synced.clone();
        loop {
            if rx.borrow_and_update().is_synced() {
                break;
            }

            rx.changed().await?;
        }

        Ok(())
    }

    /// Wait until at least the given height is synced.
    pub async fn await_at_least_synced(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<()> {
        let mut rx = self.synced.clone();
        loop {
            if rx.borrow_and_update().da_block_height() >= *height {
                break;
            }

            rx.changed().await?;
        }
        Ok(())
    }

    /// Get finalized da height that represents last block from da layer that got finalized.
    /// Panics if height is not set as of initialization of the relayer.
    pub fn get_finalized_da_height(&self) -> DaBlockHeight {
        self.synced.borrow().da_block_height()
    }
}

impl<P, D> state::EthRemote for Task<P, D>
where
    P: Middleware<Error = ProviderError>,
    D: RelayerDb + 'static,
{
    async fn finalized(&self) -> anyhow::Result<u64> {
        let mut shutdown = self.shutdown.clone();
        tokio::select! {
            biased;
            _ = shutdown.while_started() => {
                Err(anyhow::anyhow!("The relayer got a stop signal"))
            },
            block = self.eth_node.get_block(ethers_core::types::BlockNumber::Finalized) => {
                let block_number = block.map_err(|err| anyhow::anyhow!("failed to get block from Eth node: {err:?}"))?
                    .and_then(|block| block.number)
                    .ok_or(anyhow::anyhow!("Block pending"))?
                    .as_u64();
                Ok(block_number)
            }
        }
    }
}

impl<P, D> EthLocal for Task<P, D>
where
    P: Middleware<Error = ProviderError>,
    D: RelayerDb + 'static,
{
    fn observed(&self) -> u64 {
        self.synced.borrow().da_block_height().into()
    }
}

/// Creates an instance of runnable relayer service.
pub fn new_service<D>(database: D, config: Config) -> anyhow::Result<Service<D>>
where
    D: RelayerDb + 'static,
{
    let urls = config
        .relayer
        .clone()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Tried to start Relayer without setting an eth_client in the config"
            )
        })?
        .into_iter()
        .map(|url| WeightedProvider::new(Http::new(url)));

    let eth_node = Provider::new(QuorumProvider::new(Quorum::Majority, urls));
    let retry_on_error = true;
    Ok(new_service_internal(
        eth_node,
        database,
        config,
        retry_on_error,
    ))
}

#[cfg(any(test, feature = "test-helpers"))]
/// Start a test relayer.
pub fn new_service_test<P, D>(
    eth_node: P,
    database: D,
    config: Config,
) -> CustomizableService<P, D>
where
    P: Middleware<Error = ProviderError> + 'static,
    D: RelayerDb + 'static,
{
    let retry_on_fail = false;
    new_service_internal(eth_node, database, config, retry_on_fail)
}

fn new_service_internal<P, D>(
    eth_node: P,
    database: D,
    config: Config,
    retry_on_error: bool,
) -> CustomizableService<P, D>
where
    P: Middleware<Error = ProviderError> + 'static,
    D: RelayerDb + 'static,
{
    let task = NotInitializedTask::new(eth_node, database, config, retry_on_error);

    CustomizableService::new(task)
}
