use crate::{
    block_range_response::BlockRangeResponse,
    blocks::BlockSourceEvent,
    db::{
        BlockAggregatorDB,
        table::LatestBlock,
    },
    protobuf_types::Block as ProtoBlock,
    result::Error,
};
use anyhow::anyhow;
use aws_sdk_s3::{
    self,
    Client,
    primitives::ByteStream,
};
use fuel_core_storage::{
    Error as StorageError,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
    transactional::{
        Modifiable,
        StorageTransaction,
        WriteTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use prost::Message;

#[allow(non_snake_case)]
#[cfg(test)]
mod tests;

#[allow(unused)]
pub struct RemoteCache<S> {
    // aws configuration
    aws_id: String,
    aws_secret: String,
    aws_region: String,
    aws_bucket: String,
    url_base: String,
    client: Client,

    // track consistency between runs
    local_persisted: S,
    highest_new_height: Option<BlockHeight>,
    orphaned_new_height: Option<BlockHeight>,
}

impl<S> RemoteCache<S> {
    pub fn new(
        aws_id: String,
        aws_secret: String,
        aws_region: String,
        aws_bucket: String,
        url_base: String,
        client: Client,
        local_persisted: S,
    ) -> RemoteCache<S> {
        RemoteCache {
            aws_id,
            aws_secret,
            aws_region,
            aws_bucket,
            url_base,
            client,
            local_persisted,
            highest_new_height: None,
            orphaned_new_height: None,
        }
    }

    fn url_for_block(base: &str, key: &str) -> String {
        format!("{}/blocks/{}", base, key,)
    }
}

impl<S> BlockAggregatorDB for RemoteCache<S>
where
    S: Send + Sync,
    S: Modifiable,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
{
    type Block = ProtoBlock;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        block_event: BlockSourceEvent<Self::Block>,
    ) -> crate::result::Result<()> {
        let (height, block) = block_event.clone().into_inner();
        let key = block_height_to_key(&height);
        let mut buf = Vec::new();
        block.encode(&mut buf).map_err(Error::db_error)?;
        let body = ByteStream::from(buf);
        tracing::info!("Storing block in bucket: {:?}", &self.aws_bucket);
        let req = self
            .client
            .put_object()
            .bucket(&self.aws_bucket)
            .key(&key)
            .body(body)
            .content_type("application/octet-stream");
        let _ = req.send().await.map_err(Error::db_error)?;
        match block_event {
            BlockSourceEvent::NewBlock(new_height, _) => {
                tracing::debug!("New block: {:?}", new_height);
                self.highest_new_height = Some(new_height);
                if self.orphaned_new_height.is_none() {
                    self.orphaned_new_height = Some(new_height);
                }
            }
            BlockSourceEvent::OldBlock(height, _) => {
                tracing::debug!("Old block: {:?}", height);
                let mut tx = self.local_persisted.write_transaction();
                let latest_height = if height.succ() == self.orphaned_new_height {
                    self.orphaned_new_height = None;
                    self.highest_new_height.unwrap_or(height)
                } else {
                    height
                };
                tx.storage_as_mut::<LatestBlock>()
                    .insert(&(), &latest_height)
                    .map_err(|e| Error::DB(anyhow!(e)))?;
                tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;
            }
        }
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
        let base = self.url_base.clone();

        let stream = futures::stream::iter((*first..=*last).map(move |height| {
            let key = block_height_to_key(&BlockHeight::new(height));
            let url = Self::url_for_block(&base, &key);
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
        tracing::debug!("Getting current height from local cache");
        let height = self
            .local_persisted
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?;

        Ok(height.map(|b| b.into_owned()))
    }
}

pub fn block_height_to_key(height: &BlockHeight) -> String {
    format!("{:08x}", height)
}
