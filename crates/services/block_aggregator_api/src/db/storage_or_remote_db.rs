use crate::{
    block_range_response::BlockRangeResponse,
    blocks::BlockSourceEvent,
    db::{
        BlockAggregatorDB,
        remote_cache::RemoteCache,
        storage_db::StorageDB,
        table::{
            Blocks,
            Column,
            LatestBlock,
        },
    },
    result::Result,
};
use fuel_core_storage::{
    Error as StorageError,
    StorageInspect,
    StorageMutate,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Modifiable,
        StorageTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;

/// A union of a storage and a remote cache for the block aggregator. This allows both to be
/// supported in production depending on the configuration
pub enum StorageOrRemoteDB<R, S> {
    Remote(RemoteCache<R>),
    Storage(StorageDB<S>),
}

impl<R, S> StorageOrRemoteDB<R, S> {
    pub fn new_storage(storage: S) -> Self {
        StorageOrRemoteDB::Storage(StorageDB::new(storage))
    }

    pub fn new_s3(
        _storage: R,
        _aws_id: &str,
        _aws_secret: &str,
        _aws_region: &str,
        _aws_bucket: &str,
        _url_base: &str,
    ) -> Self {
        todo!("create client etc")
        // let client = {
        //     let config = aws_sdk_s3::config::Builder::new()
        //         .region(aws_region)
        //         .credentials_provider(aws_sdk_s3::Credentials::new(
        //             aws_id.clone(),
        //             aws_secret.clone(),
        //             None,
        //             None,
        //             "block-aggregator",
        //         ))
        //         .build();
        //     aws_sdk_s3::Client::from_conf(config)
        // };
        // let remote_cache = RemoteCache::new(
        //     aws_id.to_string(),
        //     aws_secret.to_string(),
        //     aws_region.to_string(),
        //     aws_bucket.to_string(),
        //     url_base.to_string(),
        //     client,
        //     storage,
        // );
        // StorageOrRemoteDB::Remote(remote_cache)
    }
}

pub fn get_env_vars() -> Option<(String, String, String, String)> {
    let aws_id = std::env::var("AWS_ACCESS_KEY_ID").ok()?;
    let aws_secret = std::env::var("AWS_SECRET_ACCESS_KEY").ok()?;
    let aws_region = std::env::var("AWS_REGION").ok()?;
    let aws_bucket = std::env::var("AWS_BUCKET").ok()?;
    Some((aws_id, aws_secret, aws_region, aws_bucket))
}

impl<R, S, T> BlockAggregatorDB for StorageOrRemoteDB<R, S>
where
    // Storage Constraints
    S: Modifiable + std::fmt::Debug,
    S: KeyValueInspect<Column = Column>,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>: StorageMutate<Blocks, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
        StorageMutate<LatestBlock, Error = StorageError>,
    S: AtomicView<LatestView = T>,
    T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + std::fmt::Debug,
    StorageTransaction<T>: StorageInspect<Blocks, Error = StorageError>,
    // Remote Constraints
    R: Send + Sync,
    R: Modifiable,
    R: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut R>:
        StorageMutate<LatestBlock, Error = StorageError>,
{
    type Block = crate::protobuf_types::Block;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(&mut self, block: BlockSourceEvent<Self::Block>) -> Result<()> {
        match self {
            StorageOrRemoteDB::Remote(remote_db) => remote_db.store_block(block).await?,
            StorageOrRemoteDB::Storage(storage_db) => {
                storage_db.store_block(block).await?
            }
        }
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<Self::BlockRangeResponse> {
        let range_response = match self {
            StorageOrRemoteDB::Remote(remote_db) => {
                remote_db.get_block_range(first, last).await?
            }
            StorageOrRemoteDB::Storage(storage_db) => {
                storage_db.get_block_range(first, last).await?
            }
        };
        Ok(range_response)
    }

    async fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let height = match self {
            StorageOrRemoteDB::Remote(remote_db) => {
                remote_db.get_current_height().await?
            }
            StorageOrRemoteDB::Storage(storage_db) => {
                storage_db.get_current_height().await?
            }
        };
        Ok(height)
    }
}
