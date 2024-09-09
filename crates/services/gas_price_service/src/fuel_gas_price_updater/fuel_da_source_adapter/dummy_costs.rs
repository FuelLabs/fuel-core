use crate::fuel_gas_price_updater::{
    service::Result as DaBlockCostsResult,
    DaBlockCosts,
    DaBlockCostsSource,
};

pub struct DummyDaBlockCosts;

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn get(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        Ok(DaBlockCosts::default())
    }
}
