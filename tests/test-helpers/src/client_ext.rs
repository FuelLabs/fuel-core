use super::*;
use cynic::QueryBuilder;
use fuel_core::upgradable_executor::native_executor::executor::max_tx_count;
use fuel_core_client::client::{
    FuelClient,
    schema::{
        BlockId,
        U32,
        block::{
            BlockByHeightArgs,
            BlockByHeightArgsFields,
            Consensus,
            Header,
        },
        schema,
        tx::OpaqueTransaction,
    },
};
use fuel_core_txpool::config::{
    HeavyWorkConfig,
    PoolLimits,
};
use fuel_core_types::fuel_types::BlockHeight;

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "../../crates/client/assets/schema.sdl",
    graphql_type = "Query",
    variables = "BlockByHeightArgs"
)]
pub struct FullBlockByHeightQuery {
    #[arguments(height: $height)]
    pub block: Option<FullBlock>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    schema_path = "../../crates/client/assets/schema.sdl",
    graphql_type = "Block"
)]
#[allow(dead_code)]
pub struct FullBlock {
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<OpaqueTransaction>,
}

#[async_trait::async_trait]
pub trait ClientExt {
    async fn full_block_by_height(
        &self,
        height: u32,
    ) -> std::io::Result<Option<FullBlock>>;
}

#[async_trait::async_trait]
impl ClientExt for FuelClient {
    async fn full_block_by_height(
        &self,
        height: u32,
    ) -> std::io::Result<Option<FullBlock>> {
        let query = FullBlockByHeightQuery::build(BlockByHeightArgs {
            height: Some(U32(height)),
        });

        let block = self.query(query).await?.block;

        Ok(block)
    }
}
