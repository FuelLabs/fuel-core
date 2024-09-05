use crate::{
    fuel_gas_price_updater,
    fuel_gas_price_updater::{
        fuel_da_source_adapter::service::{
            DaMetadataGetter,
            DaMetadataResponse,
            DaSourceService,
        },
        DaCommitDetails,
        DaCommitSource,
        Error::CouldNotFetchDARecord,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    Service,
    ServiceRunner,
};
use std::time::Duration;

pub mod block_committer_ingestor;
pub mod dummy_ingestor;
mod service;

const POLLING_INTERVAL_MS: u64 = 10_000;

// Exists to help merge the ServiceRunner and DaCommitSource traits into one type impl
pub struct FuelDaSourceService<T: DaMetadataGetter + Send + Sync + 'static>(
    ServiceRunner<DaSourceService<T>>,
);

// we decouple the DaCommitDetails that the algorithm uses with
// the responses we get from the ingestors.
impl From<DaMetadataResponse> for DaCommitDetails {
    fn from(value: DaMetadataResponse) -> Self {
        DaCommitDetails {
            l2_block_range: value.l2_block_range,
            blob_size_bytes: value.blob_size_bytes,
            blob_cost_wei: value.blob_cost,
        }
    }
}

impl<T> DaCommitSource for FuelDaSourceService<T>
where
    T: Send + Sync + Default + DaMetadataGetter,
{
    fn get_da_commit_details(
        &mut self,
    ) -> fuel_gas_price_updater::Result<Option<DaCommitDetails>> {
        let mut metadata_guard = self.0.shared.try_lock().map_err(|err| {
            CouldNotFetchDARecord(anyhow!(
                "Failed to lock shared metadata state: {:?}",
                err
            ))
        })?;

        let commit_details = metadata_guard.clone().map(Into::into);

        // now mark it as consumed because we don't want to serve the same data
        // multiple times
        *metadata_guard = None;

        Ok(commit_details)
    }
}

impl<T> FuelDaSourceService<T>
where
    T: DaMetadataGetter + Send + Sync,
{
    pub fn new(ingestor: T, polling_interval: Option<Duration>) -> anyhow::Result<Self> {
        let service = DaSourceService::new(
            ingestor,
            polling_interval.unwrap_or(Duration::from_millis(POLLING_INTERVAL_MS)),
        );
        let service_runner = ServiceRunner::new(service);
        Ok(Self(service_runner))
    }

    pub fn start(&self) -> anyhow::Result<()> {
        self.0.start()
    }

    pub fn stop(&self) -> bool {
        self.0.stop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_gas_price_updater::fuel_da_source_adapter::dummy_ingestor::DummyIngestor;

    #[derive(Default)]
    struct FakeErroringMetadataIngestor;

    #[async_trait::async_trait]
    impl DaMetadataGetter for FakeErroringMetadataIngestor {
        async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse> {
            Err(anyhow!("boo!"))
        }
    }

    #[tokio::test]
    async fn test_service_sets_cache_when_request_succeeds() {
        // given
        let mut service =
            FuelDaSourceService::new(DummyIngestor, Some(Duration::from_millis(1)))
                .unwrap();

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();

        // then
        let data_availability_metadata = service.get_da_commit_details().unwrap();
        assert!(data_availability_metadata.is_some());
    }

    #[tokio::test]
    async fn test_service_invalidates_cache() {
        // given
        let mut service =
            FuelDaSourceService::new(DummyIngestor, Some(Duration::from_millis(1)))
                .unwrap();

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();
        let _ = service.get_da_commit_details().unwrap();

        // then
        let data_availability_metadata = service.get_da_commit_details().unwrap();
        assert!(data_availability_metadata.is_none());
    }

    #[tokio::test]
    async fn test_service_does_not_set_cache_when_request_fails() {
        // given
        let mut service = FuelDaSourceService::new(
            FakeErroringMetadataIngestor,
            Some(Duration::from_millis(1)),
        )
        .unwrap();

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();

        // then
        let data_availability_metadata = service.get_da_commit_details().unwrap();
        assert!(data_availability_metadata.is_none());
    }
}
