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
use aws_config::{
    BehaviorVersion,
    default_provider::credentials::DefaultCredentialsChain,
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
pub enum StorageOrRemoteDB<S> {
    Remote(RemoteCache<S>),
    Storage(StorageDB<S>),
}

impl<S> StorageOrRemoteDB<S> {
    pub fn new_storage(storage: S, sync_from: BlockHeight) -> Self {
        StorageOrRemoteDB::Storage(StorageDB::new(storage, sync_from))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new_s3(
        storage: S,
        aws_bucket: &str,
        requester_pays: bool,
        aws_endpoint_url: Option<String>,
        sync_from: BlockHeight,
    ) -> Self {
        let credentials = DefaultCredentialsChain::builder().build().await;
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .load()
            .await;
        let mut config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);
        if let Some(endpoint) = &aws_endpoint_url {
            config_builder.set_endpoint_url(Some(endpoint.to_string()));
        }
        let config = config_builder.force_path_style(true).build();
        let client = aws_sdk_s3::Client::from_conf(config);
        let remote_cache = RemoteCache::new(
            aws_bucket.to_string(),
            requester_pays,
            aws_endpoint_url,
            client,
            storage,
            sync_from,
        )
        .await;
        StorageOrRemoteDB::Remote(remote_cache)
    }
}

impl<S, T> BlockAggregatorDB for StorageOrRemoteDB<S>
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
    S: Send + Sync,
    S: Modifiable,
    S: StorageInspect<LatestBlock, Error = StorageError>,
    for<'b> StorageTransaction<&'b mut S>:
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
