use super::scalars::U64;
use crate::fuel_core_graphql_api::Config as GraphQLConfig;
use async_graphql::{
    Context,
    Object,
};

pub struct LatestGasPrice {
    pub gas_price: U64,
    // pub block_height: u64,
}

#[Object]
impl LatestGasPrice {
    async fn gas_price(&self) -> U64 {
        self.gas_price
    }

    // async fn block_height(&self) -> u64 {
    //     self.block_height
    // }
}

#[derive(Default)]
pub struct LatestGasPriceQuery {}

#[Object]
impl LatestGasPriceQuery {
    async fn latest_gas_price(&self, ctx: &Context<'_>) -> LatestGasPrice {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        LatestGasPrice {
            gas_price: config.min_gas_price.into(),
            // block_height: config.block_height.into(),
        }
    }
}
