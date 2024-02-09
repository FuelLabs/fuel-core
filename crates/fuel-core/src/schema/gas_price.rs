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

pub struct EstimateGasPrice {
    pub gas_price: U64,
}

#[Object]
impl EstimateGasPrice {
    async fn gas_price(&self) -> U64 {
        self.gas_price
    }
}

#[derive(Default)]
pub struct EstimateGasPriceQuery {}

#[Object]
impl EstimateGasPriceQuery {
    async fn estimate_gas_price(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            desc = "Number of blocks into the future to estimate the gas price for"
        )]
        block_horizon: Option<U32>,
    ) -> async_graphql::Result<EstimateGasPrice> {
        // TODO: implement dynamic calculation based on block horizon
        //   https://github.com/FuelLabs/fuel-core/issues/1653
        let _ = block_horizon;

        let config = ctx.data_unchecked::<GraphQLConfig>();
        let gas_price = config.min_gas_price.into();

        Ok(EstimateGasPrice { gas_price })
    }
}
