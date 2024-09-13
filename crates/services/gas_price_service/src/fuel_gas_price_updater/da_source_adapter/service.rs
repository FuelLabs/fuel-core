use crate::fuel_gas_price_updater::DaBlockCosts;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    StateWatcher,
};
use std::{
    collections::HashSet,
    time::Duration,
};
use tokio::{
    sync::broadcast::Sender,
    time::{
        interval,
        Interval,
    },
};

pub use anyhow::Result;
use tokio::sync::broadcast::Receiver;

pub struct DaBlockCostsProvider<DaSource: DaBlockCostsSource + 'static> {
    handle: ServiceRunner<DaBlockCostsService<DaSource>>,
    subscription: Receiver<DaBlockCosts>,
}

#[async_trait::async_trait]
pub trait DaBlockCostsProviderPort {
    async fn start_and_await(&self) -> Result<()>;
    async fn stop_and_await(&self) -> Result<()>;
    async fn recv(&mut self) -> Result<DaBlockCosts>;
    fn try_recv(&mut self) -> Result<DaBlockCosts>;
}

#[async_trait::async_trait]
impl<DaSource> DaBlockCostsProviderPort for DaBlockCostsProvider<DaSource>
where
    DaSource: DaBlockCostsSource,
{
    async fn start_and_await(&self) -> Result<()> {
        self.handle.start_and_await().await?;
        Ok(())
    }

    async fn stop_and_await(&self) -> Result<()> {
        self.handle.stop_and_await().await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<DaBlockCosts> {
        self.subscription.recv().await.map_err(Into::into)
    }

    fn try_recv(&mut self) -> Result<DaBlockCosts> {
        self.subscription.try_recv().map_err(Into::into)
    }
}

#[derive(Clone)]
pub struct DaBlockCostsServiceSharedState {
    sender: Sender<DaBlockCosts>,
}

impl DaBlockCostsServiceSharedState {
    pub fn subscribe(&self) -> Receiver<DaBlockCosts> {
        self.sender.subscribe()
    }

    fn send(&self, da_block_costs: DaBlockCosts) -> Result<()> {
        self.sender.send(da_block_costs)?;
        Ok(())
    }
}

const POLLING_INTERVAL_MS: u64 = 10_000;

/// This struct houses the shared_state, polling interval
/// and a source, which does the actual fetching of the data
pub struct DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    poll_interval: Interval,
    sender: DaBlockCostsServiceSharedState,
    source: Source,
    cache: HashSet<DaBlockCosts>,
}

impl<Source> DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    pub fn new(source: Source, poll_interval: Option<Duration>) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(10);
        #[allow(clippy::arithmetic_side_effects)]
        Self {
            sender: DaBlockCostsServiceSharedState { sender },
            poll_interval: interval(
                poll_interval.unwrap_or(Duration::from_millis(POLLING_INTERVAL_MS)),
            ),
            source,
            cache: Default::default(),
        }
    }
}

/// This trait is implemented by the sources to obtain the
/// da block costs in a way they see fit
#[async_trait::async_trait]
pub trait DaBlockCostsSource: Send + Sync {
    async fn request_da_block_cost(&mut self) -> Result<DaBlockCosts>;
}

#[async_trait::async_trait]
impl<Source> RunnableService for DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    const NAME: &'static str = "DaBlockCostsService";

    type SharedData = DaBlockCostsServiceSharedState;

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.sender.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> Result<Self::Task> {
        self.poll_interval.reset();
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<Source> RunnableTask for DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    /// This function polls the source according to a polling interval
    /// described by the DaBlockCostsService
    async fn run(&mut self, state_watcher: &mut StateWatcher) -> Result<bool> {
        let continue_running;

        tokio::select! {
            biased;
            _ = state_watcher.while_started() => {
                continue_running = false;
            }
            _ = self.poll_interval.tick() => {
                let da_block_costs = self.source.request_da_block_cost().await?;
                if !self.cache.contains(&da_block_costs) {
                    self.cache.insert(da_block_costs.clone());
                    self.sender.send(da_block_costs)?;
                }
                continue_running = true;
            }
        }
        Ok(continue_running)
    }

    /// There are no shutdown hooks required by the sources  *yet*
    /// and they should be added here if so, in the future.
    async fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

pub fn new_provider<S: DaBlockCostsSource>(
    da_source: S,
    poll_interval: Option<Duration>,
) -> DaBlockCostsProvider<S> {
    let handle = ServiceRunner::new(DaBlockCostsService::new(da_source, poll_interval));
    let subscription = handle.shared.subscribe();
    DaBlockCostsProvider {
        handle,
        subscription,
    }
}
