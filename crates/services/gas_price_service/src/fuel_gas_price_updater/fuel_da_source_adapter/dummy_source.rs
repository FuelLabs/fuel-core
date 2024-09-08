use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaGasPriceSource,
    DaGasPriceSourceResponse,
};

pub struct DummyDaGasPriceSource;

#[async_trait::async_trait]
impl DaGasPriceSource for DummyDaGasPriceSource {
    async fn get_da_gas_price(
        &mut self,
    ) -> DaGasPriceSourceResult<DaGasPriceSourceResponse> {
        Ok(DaGasPriceSourceResponse::default())
    }
}
