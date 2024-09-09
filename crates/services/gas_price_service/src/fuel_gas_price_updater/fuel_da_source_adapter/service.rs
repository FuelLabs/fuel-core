use crate::fuel_gas_price_updater::{
    fuel_da_source_adapter::POLLING_INTERVAL_MS,
    DaBlockCosts,
    DaBlockCostsProvider,
    Error::CouldNotFetchDARecord,
};
use anyhow::anyhow;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use std::time::Duration;
use tokio::time::{
    interval,
    Interval,
};

pub type Result<T> = core::result::Result<T, anyhow::Error>;

/// This struct houses the shared_state, polling interval
/// and a source, which does the actual fetching of the data
pub struct DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    block_cost_provider: DaBlockCostsProvider,
    poll_interval: Interval,
    source: Source,
}

impl<Source> DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    pub fn new(source: Source, poll_interval: Option<Duration>) -> Self {
        Self {
            block_cost_provider: DaBlockCostsProvider::default(),
            poll_interval: interval(
                poll_interval.unwrap_or(Duration::from_millis(POLLING_INTERVAL_MS)),
            ),
            source,
        }
    }
}

/// This trait is implemented by the sources to obtain the
/// da block costs in a way they see fit
#[async_trait::async_trait]
pub trait DaBlockCostsSource: Send + Sync {
    async fn get(&mut self) -> Result<DaBlockCosts>;
}

#[async_trait::async_trait]
impl<Source> RunnableService for DaBlockCostsService<Source>
where
    Source: DaBlockCostsSource,
{
    const NAME: &'static str = "DaBlockCostsService";

    type SharedData = DaBlockCostsProvider;

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.block_cost_provider.clone()
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
    /// described by the DaSourceService
    async fn run(&mut self, state_watcher: &mut StateWatcher) -> Result<bool> {
        let continue_running;

        tokio::select! {
            biased;
            _ = state_watcher.while_started() => {
                continue_running = false;
            }
            _ = self.poll_interval.tick() => {
                let da_block_costs = self.source.get().await?;
                let mut da_block_costs_guard = self.block_cost_provider.try_lock().map_err(|err| {
                    CouldNotFetchDARecord(anyhow!(
                        "Failed to lock da block costs state: {:?}",
                     err
                     ))
                     })?;

                *da_block_costs_guard = Some(da_block_costs);

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
