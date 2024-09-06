use crate::fuel_gas_price_updater::fuel_da_source_adapter::{
    DaSharedState,
    POLLING_INTERVAL_MS,
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
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

/// This struct is used to denote the data returned
/// by da metadata providers, this can be the block committer, or some
/// other provider
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct DaMetadataResponse {
    pub l2_block_range: core::ops::Range<u32>,
    pub blob_size_bytes: u32,
    pub blob_cost: u32,
}

/// This struct houses the shared_state, polling interval
/// and a metadata_ingestor, which does the actual fetching of the data
/// we expect the ingestor to perform all the serde required
#[derive(Clone, Default)]
pub struct DaSourceService<T>
where
    T: DaMetadataGetter,
{
    shared_state: DaSharedState,
    poll_interval: Duration,
    metadata_ingestor: T,
}

impl<T> DaSourceService<T>
where
    T: DaMetadataGetter,
{
    pub fn new(metadata_ingestor: T, poll_interval: Option<Duration>) -> Self {
        Self {
            shared_state: Arc::new(Mutex::new(None)),
            poll_interval: poll_interval
                .unwrap_or(Duration::from_millis(POLLING_INTERVAL_MS)),
            metadata_ingestor,
        }
    }
}

/// This trait is implemented by metadata_ingestors to obtain the
/// da metadata in a way they see fit
#[async_trait::async_trait]
pub trait DaMetadataGetter {
    async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse>;
}

#[async_trait::async_trait]
impl<T> RunnableService for DaSourceService<T>
where
    T: DaMetadataGetter + Send + Sync,
{
    const NAME: &'static str = "DataAvailabilitySource";

    type SharedData = DaSharedState;

    type Task = Self;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl<T> RunnableTask for DaSourceService<T>
where
    T: DaMetadataGetter + Send + Sync,
{
    /// This function polls the metadata ingestor according to a polling interval
    /// described by the DaSourceService
    async fn run(&mut self, state_watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let continue_running;
        let interval = tokio::time::interval(self.poll_interval);

        tokio::pin!(interval);
        tokio::select! {
            biased;
            _ = state_watcher.while_started() => {
                continue_running = false;
            }
            _ = interval.tick() => {
                let metadata_response = self.metadata_ingestor.get_da_metadata().await?;
                let mut cached_metadata_response = self.shared_state.lock().await;
                *cached_metadata_response = Some(metadata_response);
                continue_running = true;
            }
        }
        Ok(continue_running)
    }

    /// There are no shutdown hooks required by the metadata ingestors *yet*
    /// and they should be added here if so, in the future.
    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}
