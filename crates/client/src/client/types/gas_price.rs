use crate::client::schema;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Debug, Copy, Clone)]
pub struct LatestGasPrice {
    pub gas_price: u64,
    pub block_height: BlockHeight,
}

// GraphQL Translation
impl From<schema::gas_price::LatestGasPrice> for LatestGasPrice {
    fn from(value: schema::gas_price::LatestGasPrice) -> Self {
        Self {
            gas_price: value.gas_price.into(),
            block_height: BlockHeight::new(value.block_height.into()),
        }
    }
}

pub struct EstimateGasPrice {
    pub gas_price: u64,
}

impl From<schema::gas_price::EstimateGasPrice> for EstimateGasPrice {
    fn from(value: schema::gas_price::EstimateGasPrice) -> Self {
        Self {
            gas_price: value.gas_price.into(),
        }
    }
}
