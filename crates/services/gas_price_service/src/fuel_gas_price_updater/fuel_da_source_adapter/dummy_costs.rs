use crate::fuel_gas_price_updater::{
    service::Result as DaBlockCostsResult,
    DaBlockCosts,
    DaBlockCostsSource,
};

pub struct DummyDaBlockCosts;

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        Ok(DaBlockCosts::default())
    }
}
