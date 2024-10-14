use super::{
    scalars::HexString,
    ReadViewProvider,
};
use crate::{
    fuel_core_graphql_api::{
        query_costs,
        IntoApiResult,
    },
    schema::scalars::U32,
};
use async_graphql::{
    Context,
    Object,
};

pub struct DaCompressedBlock {
    bytes: Vec<u8>,
}

impl From<Vec<u8>> for DaCompressedBlock {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

#[Object]
impl DaCompressedBlock {
    async fn bytes(&self) -> HexString {
        HexString(self.bytes.clone())
    }
}

#[derive(Default)]
pub struct DaCompressedBlockQuery;

#[Object]
impl DaCompressedBlockQuery {
    #[graphql(complexity = "query_costs().da_compressed_block_read")]
    async fn da_compressed_block(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Height of the block")] height: U32,
    ) -> async_graphql::Result<Option<DaCompressedBlock>> {
        let query = ctx.read_view()?;
        query
            .da_compressed_block(&height.0.into())
            .into_api_result()
    }
}
