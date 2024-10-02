use fuel_core_services::{
    RunnableService,
    RunnableTask,
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

use crate::v1::da_source_adapter::DaBlockCosts;
pub use anyhow::Result;

#[derive(Clone)]
pub struct DaBlockCostsSharedState(Sender<DaBlockCosts>);

impl DaBlockCostsSharedState {
    fn new(sender: Sender<DaBlockCosts>) -> Self {
        Self(sender)
    }
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DaBlockCosts> {
        self.0.subscribe()
    }
}

/// This struct houses the shared_state, polling interval
/// and a source, which does the actual fetching of the data
pub struct DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    poll_interval: Interval,
    source: Source,
    shared_state: DaBlockCostsSharedState,
    cache: HashSet<DaBlockCosts>,
}

const DA_BLOCK_COSTS_CHANNEL_SIZE: usize = 10;
const POLLING_INTERVAL_MS: u64 = 10_000;

impl<Source> DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    pub fn new(source: Source, poll_interval: Option<Duration>) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(DA_BLOCK_COSTS_CHANNEL_SIZE);
        #[allow(clippy::arithmetic_side_effects)]
        Self {
            shared_state: DaBlockCostsSharedState::new(sender),
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

    type SharedData = DaBlockCostsSharedState;

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
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
                    self.shared_state.0.send(da_block_costs)?;
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

pub fn new_service<S: DaBlockCostsSource>(
    da_source: S,
    poll_interval: Option<Duration>,
) -> ServiceRunner<DaBlockCostsService<S>> {
    ServiceRunner::new(DaBlockCostsService::new(da_source, poll_interval))
}
