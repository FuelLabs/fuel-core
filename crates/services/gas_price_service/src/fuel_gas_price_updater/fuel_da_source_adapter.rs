use crate::{
    fuel_gas_price_updater,
    fuel_gas_price_updater::{
        fuel_da_source_adapter::service::DaMetadataResponse,
        DaCommitDetails,
        DaCommitSource,
        Error::CouldNotFetchDARecord,
    },
};
use anyhow::anyhow;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod block_committer_ingestor;
pub mod dummy_ingestor;
pub mod service;

pub const POLLING_INTERVAL_MS: u64 = 10_000;

pub type DaSharedState = Arc<Mutex<Option<DaMetadataResponse>>>;

impl DaCommitSource for DaSharedState {
    fn get_da_commit_details(
        &mut self,
    ) -> fuel_gas_price_updater::Result<Option<DaCommitDetails>> {
        let mut metadata_guard = self.try_lock().map_err(|err| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_gas_price_updater::fuel_da_source_adapter::service::{
        DaMetadataGetter,
        DaSourceService,
    };
    use fuel_core_services::{
        RunnableService,
        Service,
        ServiceRunner,
    };
    use std::time::Duration;

    #[derive(Default)]
    struct FakeMetadataIngestor;

    #[async_trait::async_trait]
    impl DaMetadataGetter for FakeMetadataIngestor {
        async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse> {
            Ok(DaMetadataResponse::default())
        }
    }

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
        let service =
            DaSourceService::new(FakeMetadataIngestor, Some(Duration::from_millis(1)));

        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();

        // then
        let da_commit_details = shared_state.get_da_commit_details().unwrap();
        assert!(da_commit_details.is_some());
    }

    #[tokio::test]
    async fn test_service_invalidates_cache() {
        // given
        let service =
            DaSourceService::new(FakeMetadataIngestor, Some(Duration::from_millis(1)));
        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();
        let _ = shared_state.get_da_commit_details().unwrap();

        // then
        let da_commit_details = shared_state.get_da_commit_details().unwrap();
        assert!(da_commit_details.is_none());
    }

    #[tokio::test]
    async fn test_service_does_not_set_cache_when_request_fails() {
        // given
        let service = DaSourceService::new(
            FakeErroringMetadataIngestor,
            Some(Duration::from_millis(1)),
        );
        let mut shared_state = service.shared_data();
        let service = ServiceRunner::new(service);

        // when
        service.start().unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        service.stop();

        // then
        let da_commit_details = shared_state.get_da_commit_details().unwrap();
        assert!(da_commit_details.is_none());
    }
}
