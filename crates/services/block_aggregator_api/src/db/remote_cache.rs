use crate::{
    block_range_response::BlockRangeResponse,
    blocks::BlockBytes,
    db::BlockAggregatorDB,
    protobuf_types::Block as ProtoBlock,
    result::Error,
};
use aws_sdk_s3::{
    self,
    Client,
    primitives::ByteStream,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
};
use prost::Message;

#[allow(unused)]
pub struct RemoteCache {
    aws_id: String,
    aws_secret: String,
    aws_region: String,
    aws_bucket: String,
    client: Client,
    head: Option<BlockHeight>,
}

impl RemoteCache {
    pub fn new(
        aws_id: String,
        aws_secret: String,
        aws_region: String,
        aws_bucket: String,
        client: Client,
    ) -> RemoteCache {
        RemoteCache {
            aws_id,
            aws_secret,
            aws_region,
            aws_bucket,
            client,
        }
    }
}

impl BlockAggregatorDB for RemoteCache {
    type Block = ProtoBlock;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        height: BlockHeight,
        block: ProtoBlock,
    ) -> crate::result::Result<()> {
        let key = block_height_to_key(&height);
        let mut buf = Vec::new();
        block.encode(&mut buf).map_err(Error::db_error)?;
        let body = ByteStream::from(buf);
        let req = self
            .client
            .put_object()
            .bucket(&self.aws_bucket)
            .key(&key)
            .body(body)
            .content_type("application/octet-stream");
        let _ = req.send().await.map_err(Error::db_error)?;
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> crate::result::Result<Self::BlockRangeResponse> {
        todo!()
    }

    async fn get_current_height(&self) -> crate::result::Result<BlockHeight> {
        todo!()
    }
}

pub fn block_height_to_key(height: &BlockHeight) -> String {
    format!("{:08x}", height)
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::importer_and_db_source::{
        BlockSerializer,
        serializer_adapter::SerializerAdapter,
    };
    use aws_sdk_s3::{
        operation::{
            get_object::GetObjectOutput,
            put_object::PutObjectOutput,
        },
        primitives::ByteStream,
    };
    use aws_smithy_mocks::{
        RuleMode,
        mock,
        mock_client,
    };

    fn arb_proto_block() -> ProtoBlock {
        let block = FuelBlock::default();
        let mut serializer = SerializerAdapter;
        let proto_block = serializer.serialize_block(&block).unwrap();
        proto_block
    }

    #[tokio::test]
    async fn store_block__happy_path() {
        let put_happy_rule = mock!(Client::put_object)
            .match_requests(|req| req.bucket() == Some("test-bucket"))
            .sequence()
            .output(|| PutObjectOutput::builder().build())
            .build();
        // given
        let client = mock_client!(aws_sdk_s3, [&put_happy_rule]);
        let aws_id = "test-id".to_string();
        let aws_secret = "test-secret".to_string();
        let aws_region = "test-region".to_string();
        let aws_bucket = "test-bucket".to_string();
        let mut adapter =
            RemoteCache::new(aws_id, aws_secret, aws_region, aws_bucket, client);
        let block_height = BlockHeight::new(123);
        let block = arb_proto_block();

        // when
        let res = adapter.store_block(block_height, block).await;

        // then
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn get_block_range__happy_path() {
        todo!()
    }
}
