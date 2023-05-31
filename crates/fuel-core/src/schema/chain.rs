use crate::{
    fuel_core_graphql_api::{
        service::Database,
        Config as GraphQLConfig,
    },
    query::{
        BlockQueryData,
        ChainQueryData,
    },
    schema::{
        block::Block,
        scalars::{
            U32,
            U64,
        },
    },
};
use async_graphql::{
    Context,
    Object,
};
use fuel_core_types::fuel_tx;

pub struct ChainInfo;

pub struct ConsensusParameters(fuel_tx::ConsensusParameters);

#[Object]
impl ConsensusParameters {
    async fn contract_max_size(&self) -> U64 {
        self.0.contract_max_size.into()
    }

    async fn max_inputs(&self) -> U64 {
        self.0.max_inputs.into()
    }

    async fn max_outputs(&self) -> U64 {
        self.0.max_outputs.into()
    }

    async fn max_witnesses(&self) -> U64 {
        self.0.max_witnesses.into()
    }

    async fn max_gas_per_tx(&self) -> U64 {
        self.0.max_gas_per_tx.into()
    }

    async fn max_script_length(&self) -> U64 {
        self.0.max_script_length.into()
    }

    async fn max_script_data_length(&self) -> U64 {
        self.0.max_script_data_length.into()
    }

    async fn max_storage_slots(&self) -> U64 {
        self.0.max_storage_slots.into()
    }

    async fn max_predicate_length(&self) -> U64 {
        self.0.max_predicate_length.into()
    }

    async fn max_predicate_data_length(&self) -> U64 {
        self.0.max_predicate_data_length.into()
    }

    async fn max_gas_per_predicate(&self) -> U64 {
        self.0.max_gas_per_predicate.into()
    }

    async fn gas_price_factor(&self) -> U64 {
        self.0.gas_price_factor.into()
    }

    async fn gas_per_byte(&self) -> U64 {
        self.0.gas_per_byte.into()
    }

    async fn max_message_data_length(&self) -> U64 {
        self.0.max_message_data_length.into()
    }

    async fn chain_id(&self) -> U64 {
        (*self.0.chain_id).into()
    }
}

#[Object]
impl ChainInfo {
    async fn name(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let data: &Database = ctx.data_unchecked();
        Ok(data.name()?)
    }

    async fn latest_block(&self, ctx: &Context<'_>) -> async_graphql::Result<Block> {
        let query: &Database = ctx.data_unchecked();

        let latest_block = query.latest_block()?.into();
        Ok(latest_block)
    }

    async fn base_chain_height(&self, ctx: &Context<'_>) -> U32 {
        let query: &Database = ctx.data_unchecked();

        let height = query
            .latest_block_height()
            .expect("The blockchain always should have genesis block");

        height.into()
    }

    async fn peer_count(&self) -> u16 {
        0
    }

    async fn consensus_parameters(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ConsensusParameters> {
        let config = ctx.data_unchecked::<GraphQLConfig>();

        Ok(ConsensusParameters(config.transaction_parameters))
    }
}

#[derive(Default)]
pub struct ChainQuery;

#[Object]
impl ChainQuery {
    async fn chain(&self) -> ChainInfo {
        ChainInfo
    }
}
