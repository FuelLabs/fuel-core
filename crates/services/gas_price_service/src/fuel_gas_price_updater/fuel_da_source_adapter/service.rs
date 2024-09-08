use crate::fuel_gas_price_updater::{
    fuel_da_source_adapter::POLLING_INTERVAL_MS,
    DaGasPriceCommit,
    SetDaGasPriceToSink,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::time::Duration;
use tokio::time::{
    interval,
    Interval,
};

pub type Result<T> = core::result::Result<T, anyhow::Error>;

/// This struct is used to denote the data returned
/// by da metadata providers, this can be the block committer, or some
/// other provider
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct DaGasPriceSourceResponse {
    pub l2_block_range: core::ops::Range<u32>,
    pub blob_size_bytes: u32,
    pub blob_cost: u32,
}

/// This struct houses the shared_state, polling interval
/// and a source, which does the actual fetching of the data
pub struct DaGasPriceProviderService<Source, Sink>
where
    Source: DaGasPriceSource,
    Sink: SetDaGasPriceToSink,
{
    sink: Sink,
    poll_interval: Interval,
    source: Source,
}

impl<Source, Sink> DaGasPriceProviderService<Source, Sink>
where
    Source: DaGasPriceSource,
    Sink: SetDaGasPriceToSink,
{
    pub fn new(source: Source, poll_interval: Option<Duration>) -> Self {
        Self {
            sink: Sink::default(),
            poll_interval: interval(
                poll_interval.unwrap_or(Duration::from_millis(POLLING_INTERVAL_MS)),
            ),
            source,
        }
    }
}

/// This trait is implemented by the sources to obtain the
/// da metadata in a way they see fit
#[async_trait::async_trait]
pub trait DaGasPriceSource: Send + Sync {
    async fn get_da_gas_price(&mut self) -> Result<DaGasPriceSourceResponse>;
}

#[async_trait::async_trait]
impl<Source, Sink> RunnableService for DaGasPriceProviderService<Source, Sink>
where
    Source: DaGasPriceSource,
    Sink: SetDaGasPriceToSink,
{
    const NAME: &'static str = "DaGasPriceProviderService";

    type SharedData = Sink;

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.sink.clone()
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

// we decouple the DaGasPriceCommit that the algorithm uses with
// the responses we get from the sources.
impl From<DaGasPriceSourceResponse> for DaGasPriceCommit {
    fn from(value: DaGasPriceSourceResponse) -> Self {
        DaGasPriceCommit {
            l2_block_range: value.l2_block_range,
            blob_size_bytes: value.blob_size_bytes,
            blob_cost_wei: value.blob_cost,
        }
    }
}

#[async_trait::async_trait]
impl<Source, Sink> RunnableTask for DaGasPriceProviderService<Source, Sink>
where
    Source: DaGasPriceSource,
    Sink: SetDaGasPriceToSink,
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
                let metadata_response = self.source.get_da_gas_price().await?;
                self.sink.set_da_commit(metadata_response.into())?;
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
