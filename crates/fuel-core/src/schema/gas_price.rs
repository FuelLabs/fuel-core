use super::scalars::{
    U32,
    U64,
};
use crate::{
    graphql_api::{
        api_service::GasPriceProvider,
        query_costs,
    },
    schema::ReadViewProvider,
};
use async_graphql::{
    Context,
    Object,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::{
        Transaction,
        field::MintGasPrice,
    },
};

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
    #[graphql(complexity = "query_costs().block_header")]
    async fn latest_gas_price(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<LatestGasPrice> {
        let query = ctx.read_view()?;

        let latest_block: Block<_> = query.latest_block()?;
        let block_height: u32 = (*latest_block.header().height()).into();
        let mut gas_price: U64 = 0.into();
        if let Some(tx_id) = latest_block.transactions().last() {
            if let Transaction::Mint(mint_tx) = query.transaction(tx_id)? {
                gas_price = (*mint_tx.gas_price()).into();
            }
        }
        Ok(LatestGasPrice {
            gas_price,
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
    #[graphql(complexity = "2 * query_costs().storage_read")]
    async fn estimate_gas_price(
        &self,
        ctx: &Context<'_>,
        #[graphql(
            desc = "Number of blocks into the future to estimate the gas price for"
        )]
        block_horizon: Option<U32>,
    ) -> async_graphql::Result<EstimateGasPrice> {
        let block_horizon = block_horizon.map(|h| h.0);

        let gas_price = ctx.estimate_gas_price(block_horizon)?;

        Ok(EstimateGasPrice {
            gas_price: gas_price.into(),
        })
    }
}

pub trait EstimateGasPriceExt {
    fn estimate_gas_price(
        &self,
        block_horizon: Option<u32>,
    ) -> async_graphql::Result<u64>;
}

impl EstimateGasPriceExt for Context<'_> {
    fn estimate_gas_price(
        &self,
        block_horizon: Option<u32>,
    ) -> async_graphql::Result<u64> {
        let query = self.read_view()?;

        let latest_block_height: u32 = query.latest_block_height()?.into();
        let target_block = block_horizon
            .and_then(|h| h.checked_add(latest_block_height))
            .ok_or(async_graphql::Error::new(format!(
                "Invalid block horizon. Overflows latest block :{latest_block_height:?}"
            )))?;

        let gas_price_provider = self.data_unchecked::<GasPriceProvider>();
        let gas_price = gas_price_provider
            .worst_case_gas_price(target_block.into())
            .ok_or(async_graphql::Error::new(format!(
                "Failed to estimate gas price for block, algorithm not yet set: {target_block:?}"
            )))?;

        Ok(gas_price)
    }
}
