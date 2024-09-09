use crate::fuel_gas_price_updater::{
    da_source_adapter::service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};

pub struct DummyDaBlockCosts;

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        Ok(DaBlockCosts::default())
    }
}
