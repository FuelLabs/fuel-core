use crate::fuel_gas_price_updater::{
    da_source_adapter::service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};

pub struct DummyDaBlockCosts {
    value: DaBlockCostsResult<DaBlockCosts>,
}

impl DummyDaBlockCosts {
    pub fn new(value: DaBlockCostsResult<DaBlockCosts>) -> Self {
        Self { value }
    }
}

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        match &self.value {
            Ok(da_block_costs) => Ok(da_block_costs.clone()),
            Err(err) => Err(anyhow::anyhow!(err.to_string())),
        }
    }
}
