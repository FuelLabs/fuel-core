use crate::fuel_gas_price_updater::fuel_da_source_adapter::service::{
    DaMetadataGetter,
    DaMetadataResponse,
};

pub struct DummyIngestor;

#[async_trait::async_trait]
impl DaMetadataGetter for DummyIngestor {
    async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse> {
        Ok(DaMetadataResponse::default())
    }
}
