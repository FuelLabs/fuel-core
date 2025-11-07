use crate::{
    block_range_response::BlockRangeResponse,
    blocks::{
        BlockBytes,
        BlockSourceEvent,
    },
    db::BlockAggregatorDB,
    protobuf_types::{
        Block as ProtoBlock,
        Block,
    },
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

#[allow(non_snake_case)]
#[cfg(test)]
mod tests;

#[allow(unused)]
pub struct RemoteCache<S> {
    aws_id: String,
    aws_secret: String,
    aws_region: String,
    aws_bucket: String,
    client: Client,
    local_persisted: S,
}

impl<S> RemoteCache<S> {
    pub fn new(
        aws_id: String,
        aws_secret: String,
        aws_region: String,
        aws_bucket: String,
        client: Client,
        local_persisted: S,
    ) -> RemoteCache<S> {
        RemoteCache {
            aws_id,
            aws_secret,
            aws_region,
            aws_bucket,
            client,
            local_persisted,
        }
    }
}

impl<S: Send + Sync> BlockAggregatorDB for RemoteCache<S> {
    type Block = ProtoBlock;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        block: BlockSourceEvent<Self::Block>,
    ) -> crate::result::Result<()> {
        let (height, block) = match block {
            BlockSourceEvent::NewBlock(height, block) => {
                // Do nothing extra
                (height, block)
            }
            BlockSourceEvent::OldBlock(height, block) => {
                // TODO: record latest block
                (height, block)
            }
        };
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
        // TODO: Check if it exists
        let region = self.aws_region.clone();
        let bucket = self.aws_bucket.clone();

        let stream = futures::stream::iter((*first..=*last).map(move |height| {
            let key = block_height_to_key(&BlockHeight::new(height));
            let url = "todo".to_string();
            crate::block_range_response::RemoteBlockRangeResponse {
                region: region.clone(),
                bucket: bucket.clone(),
                key: key.clone(),
                url,
            }
        }));
        Ok(BlockRangeResponse::Remote(Box::pin(stream)))
    }

    async fn get_current_height(&self) -> crate::result::Result<Option<BlockHeight>> {
        todo!()
    }
}

pub fn block_height_to_key(height: &BlockHeight) -> String {
    format!("{:08x}", height)
}
