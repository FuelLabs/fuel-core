use super::scalars::{
    U32,
    U64,
};
use crate::{
    fuel_core_graphql_api::database::ReadView,
    graphql_api::api_service::GasPriceProvider,
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
        let gas_price_provider = ctx.data_unchecked::<GasPriceProvider>();
        let query: &ReadView = ctx.data_unchecked();

        let latest_block: Block<_> = query.latest_block()?;
        let block_height = u32::from(*latest_block.header().height());
        let gas_price = gas_price_provider
            .known_gas_price(block_height.into())
            .await
            .ok_or(async_graphql::Error::new(format!(
                "Gas price not found for latest block:{block_height:?}"
            )))?;

        Ok(LatestGasPrice {
            gas_price: gas_price.into(),
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
        let query: &ReadView = ctx.data_unchecked();

        let latest_block: Block<_> = query.latest_block()?;
        let latest_block_height = u32::from(*latest_block.header().height());
        let target_block = block_horizon
            .and_then(|h| h.0.checked_add(latest_block_height))
            .ok_or(async_graphql::Error::new(format!(
                "Invalid block horizon. Overflows latest block :{latest_block_height:?}"
            )))?;

        let gas_price_provider = ctx.data_unchecked::<GasPriceProvider>();
        let gas_price = gas_price_provider
            .worst_case_gas_price(target_block.into())
            .await;

        Ok(EstimateGasPrice {
            gas_price: gas_price.into(),
        })
    }
}
