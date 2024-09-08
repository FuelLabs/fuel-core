use crate::fuel_gas_price_updater::{
    fuel_da_source_adapter::POLLING_INTERVAL_MS,
    DaGasPrice,
    SetDaGasPriceToSink,
};
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
/// da gas price in a way they see fit
#[async_trait::async_trait]
pub trait DaGasPriceSource: Send + Sync {
    async fn get(&mut self) -> Result<DaGasPrice>;
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
                let gas_price = self.source.get().await?;
                self.sink.set(gas_price)?;
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
