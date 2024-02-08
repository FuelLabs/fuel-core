use crate::client::schema;

pub struct LatestGasPrice {
    pub gas_price: u64,
    pub block_height: u32,
}

// GraphQL Translation
impl From<schema::gas_price::LatestGasPrice> for LatestGasPrice {
    fn from(value: schema::gas_price::LatestGasPrice) -> Self {
        Self {
            gas_price: value.gas_price.into(),
            block_height: value.block_height.into(),
        }
    }
}
