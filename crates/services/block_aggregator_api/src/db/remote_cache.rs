use crate::{
    block_range_response::BlockRangeResponse,
    db::{
        BlocksProvider,
        BlocksStorage,
        table::{
            Column,
            LatestBlock,
            Mode,
        },
    },
    result::Error,
};
use anyhow::anyhow;
use aws_sdk_s3::{
    self,
    Client,
    primitives::ByteStream,
};
use flate2::{
    Compression,
    write::GzEncoder,
};
use fuel_core_storage::{
    StorageAsMut,
    StorageAsRef,
    kv_store::KeyValueInspect,
    structured_storage::AsStructuredStorage,
    transactional::{
        Modifiable,
        WriteTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use std::{
    io::Write,
    sync::Arc,
};

#[allow(non_snake_case)]
#[cfg(test)]
mod tests;

pub struct RemoteCache<S> {
    // aws configuration
    aws_bucket: String,
    client: Client,
    publishes_blocks: bool,

    // track consistency between runs
    local_persisted: S,
}

impl<S> RemoteCache<S> {
    pub fn new(
        aws_bucket: String,
        client: Client,
        local_persisted: S,
        publish: bool,
    ) -> RemoteCache<S> {
        RemoteCache {
            aws_bucket,
            client,
            publishes_blocks: publish,
            local_persisted,
        }
    }
}

pub struct RemoteBlocksProvider<S> {
    // aws configuration
    aws_bucket: String,
    requester_pays: bool,
    aws_endpoint: Option<String>,

    // track consistency between runs
    local_persisted: S,
}

impl<S> RemoteBlocksProvider<S> {
    pub fn new(
        aws_bucket: String,
        requester_pays: bool,
        aws_endpoint: Option<String>,
        local_persisted: S,
    ) -> Self {
        RemoteBlocksProvider {
            aws_bucket,
            requester_pays,
            aws_endpoint,
            local_persisted,
        }
    }

    fn stream_blocks(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> crate::result::Result<BlockRangeResponse> {
        let bucket = self.aws_bucket.clone();
        let requester_pays = self.requester_pays;
        let aws_endpoint = self.aws_endpoint.clone();
        let stream = futures::stream::iter((*first..=*last).map(move |height| {
            let block_height = BlockHeight::new(height);
            let key = block_height_to_key(&block_height);
            let res = crate::block_range_response::RemoteS3Response {
                bucket: bucket.clone(),
                key: key.clone(),
                requester_pays,
                aws_endpoint: aws_endpoint.clone(),
            };
            (block_height, res)
        }));
        Ok(BlockRangeResponse::S3(Box::pin(stream)))
    }
}

impl<S> BlocksProvider for RemoteBlocksProvider<S>
where
    S: Send + Sync + 'static,
    S: KeyValueInspect<Column = Column>,
{
    type Block = Arc<[u8]>;
    type BlockRangeResponse = BlockRangeResponse;

    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> crate::result::Result<Self::BlockRangeResponse> {
        let current_height = self.get_current_height()?.unwrap_or(BlockHeight::new(0));
        if last > current_height {
            Err(Error::db_error(anyhow!(
                "Requested block height {} is greater than current synced height {}",
                last,
                current_height
            )))
        } else {
            self.stream_blocks(first, last)
        }
    }

    fn get_current_height(&self) -> crate::result::Result<Option<BlockHeight>> {
        let height = self
            .local_persisted
            .as_structured_storage()
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?
            .map(|b| b.height());

        Ok(height)
    }
}

impl<S> BlocksStorage for RemoteCache<S>
where
    S: Send + Sync,
    S: Modifiable,
    S: KeyValueInspect<Column = Column>,
{
    type Block = Arc<[u8]>;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        height: BlockHeight,
        block: &Self::Block,
    ) -> crate::result::Result<()> {
        let current_height = self.get_current_height()?;

        if let Some(current_height) = current_height
            && let Some(next_height) = current_height.succ()
            && next_height != height
        {
            return Err(Error::db_error(anyhow!(
                "Cannot store block at height {}: current height is {}, expected next height is {}",
                height,
                current_height,
                next_height
            )));
        }

        let key = block_height_to_key(&height);
        let zipped = gzip_bytes(block)?;
        let body = ByteStream::from(zipped);
        if self.publishes_blocks {
            let req = self
                .client
                .put_object()
                .bucket(&self.aws_bucket)
                .key(&key)
                .body(body)
                .content_encoding("gzip")
                .content_type("application/grpc-web");
            let _ = req.send().await.map_err(Error::db_error)?;
        }

        let mut tx = self.local_persisted.write_transaction();
        tx.storage_as_mut::<LatestBlock>()
            .insert(&(), &Mode::new_s3(height))
            .map_err(|e| Error::DB(anyhow!(e)))?;
        tx.commit().map_err(|e| Error::DB(anyhow!(e)))?;

        Ok(())
    }
}

impl<S> RemoteCache<S>
where
    S: Send + Sync,
    S: KeyValueInspect<Column = Column>,
{
    pub fn get_current_height(&self) -> crate::result::Result<Option<BlockHeight>> {
        let height = self
            .local_persisted
            .as_structured_storage()
            .storage_as_ref::<LatestBlock>()
            .get(&())
            .map_err(|e| Error::DB(anyhow!(e)))?
            .map(|b| b.height());

        Ok(height)
    }
}

pub fn block_height_to_key(height: &BlockHeight) -> String {
    let raw: [u8; 4] = height.to_bytes();
    format!(
        "{:02}/{:02}/{:02}/{:02}",
        &raw[0], &raw[1], &raw[2], &raw[3]
    )
}

pub fn gzip_bytes(data: &[u8]) -> crate::result::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(Error::db_error)?;
    encoder.finish().map_err(Error::db_error)
}
