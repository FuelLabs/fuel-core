use crate::fuel_gas_price_updater::fuel_da_source_adapter::service::{
    DaMetadataGetter,
    DaMetadataResponse,
};
use std::time::Duration;

pub struct DummyIngestor;

#[async_trait::async_trait]
impl DaMetadataGetter for DummyIngestor {
    async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse> {
        // acts like its doing some work, so that the polling interval ticks are not too fast
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(DaMetadataResponse::default())
    }
}
