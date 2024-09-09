use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaBlockCosts,
    DaBlockCostsSource,
};

pub struct DummyDaBlockCosts;

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn get(&mut self) -> DaGasPriceSourceResult<DaBlockCosts> {
        Ok(DaBlockCosts::default())
    }
}
