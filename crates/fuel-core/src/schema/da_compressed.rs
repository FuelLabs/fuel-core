use super::scalars::HexString;
use crate::{
    fuel_core_graphql_api::{
        IntoApiResult,
        query_costs,
    },
    graphql_api::api_service::DaCompressionProvider,
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
        HexString(self.bytes.clone().into())
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
        let da_compression_provider = ctx.data_unchecked::<DaCompressionProvider>();
        da_compression_provider
            .da_compressed_block(&height.0.into())
            .into_api_result()
    }
}
