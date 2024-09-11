use crate::fuel_gas_price_updater::{
    da_source_adapter::POLLING_INTERVAL_MS,
    DaBlockCosts,
};
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
    sync::mpsc::Sender,
    time::{
        interval,
        Interval,
    },
};

pub use anyhow::Result;

/// This struct houses the shared_state, polling interval
/// and a source, which does the actual fetching of the data
pub struct DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    poll_interval: Interval,
    source: Source,
    sender: Sender<DaBlockCosts>,
    cache: HashSet<DaBlockCosts>,
}

impl<Source> DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    pub fn new(
        source: Source,
        sender: Sender<DaBlockCosts>,
        poll_interval: Option<Duration>,
    ) -> Self {
        #[allow(clippy::arithmetic_side_effects)]
        Self {
            sender,
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

    type SharedData = ();

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {}

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
                    self.sender.send(da_block_costs).await?;
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
    sender: Sender<DaBlockCosts>,
    poll_interval: Option<Duration>,
) -> ServiceRunner<DaBlockCostsService<S>> {
    ServiceRunner::new(DaBlockCostsService::new(da_source, sender, poll_interval))
}
