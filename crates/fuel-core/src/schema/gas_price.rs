use super::scalars::{
    U32,
    U64,
};
use crate::{
    fuel_core_graphql_api::{
        database::ReadView,
        Config as GraphQLConfig,
    },
    query::BlockQueryData,
};
use async_graphql::{
    Context,
    Object,
};
use fuel_core_types::blockchain::block::Block;

pub struct LatestGasPrice {
    pub gas_price: U64,
    pub block_height: U32,
}

#[Object]
impl LatestGasPrice {
    async fn gas_price(&self) -> U64 {
        self.gas_price
    }

    async fn block_height(&self) -> U32 {
        self.block_height
    }
}

#[derive(Default)]
pub struct LatestGasPriceQuery {}

#[Object]
impl LatestGasPriceQuery {
    async fn latest_gas_price(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<LatestGasPrice> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        let query: &ReadView = ctx.data_unchecked();
        let latest_block: Block<_> = query.latest_block()?;
        let block_height = u32::from(*latest_block.header().height());

        Ok(LatestGasPrice {
            gas_price: config.min_gas_price.into(),
            block_height: block_height.into(),
        })
    }
}
