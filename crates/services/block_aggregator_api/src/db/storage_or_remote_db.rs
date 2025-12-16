use crate::{
    block_range_response::BlockRangeResponse,
    db::{
        BlocksProvider,
        BlocksStorage,
        remote_cache::{
            RemoteBlocksProvider,
            RemoteCache,
        },
        storage_db::{
            StorageBlocksProvider,
            StorageDB,
        },
        table::Column,
    },
    result::Result,
};
use aws_config::{
    BehaviorVersion,
    default_provider::credentials::DefaultCredentialsChain,
};
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Modifiable,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;

/// A union of a storage and a remote cache for the block aggregator. This allows both to be
/// supported in production depending on the configuration
pub enum StorageOrRemoteBlocksProvider<S> {
    Remote(RemoteBlocksProvider<S>),
    Storage(StorageBlocksProvider<S>),
}

impl<S> StorageOrRemoteBlocksProvider<S> {
    pub fn new_storage(storage: S) -> Self {
        StorageOrRemoteBlocksProvider::Storage(StorageBlocksProvider::new(storage))
    }

    pub fn new_s3(
        storage: S,
        aws_bucket: String,
        requester_pays: bool,
        aws_endpoint_url: Option<String>,
    ) -> Self {
        let remote_cache = RemoteBlocksProvider::new(
            aws_bucket,
            requester_pays,
            aws_endpoint_url,
            storage,
        );
        StorageOrRemoteBlocksProvider::Remote(remote_cache)
    }
}

/// A union of a storage and a remote cache for the block aggregator. This allows both to be
/// supported in production depending on the configuration
pub enum StorageOrRemoteDB<S> {
    Remote(RemoteCache<S>),
    Storage(StorageDB<S>),
}

impl<S> StorageOrRemoteDB<S> {
    pub fn new_storage(storage: S) -> Self {
        StorageOrRemoteDB::Storage(StorageDB::new(storage))
    }

    pub async fn new_s3(
        storage: S,
        aws_bucket: String,
        aws_endpoint_url: Option<String>,
        publish: bool,
    ) -> Self {
        let credentials = DefaultCredentialsChain::builder().build().await;
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials)
            .load()
            .await;
        let mut config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);
        if let Some(endpoint) = aws_endpoint_url {
            config_builder.set_endpoint_url(Some(endpoint));
        }
        let config = config_builder.force_path_style(true).build();
        let client = aws_sdk_s3::Client::from_conf(config);
        let remote_cache = RemoteCache::new(aws_bucket, client, storage, publish);
        StorageOrRemoteDB::Remote(remote_cache)
    }
}

impl<S> BlocksStorage for StorageOrRemoteDB<S>
where
    S: Modifiable + Send + Sync,
    S: KeyValueInspect<Column = Column>,
{
    type Block = Arc<Vec<u8>>;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        block_height: BlockHeight,
        block: &Self::Block,
    ) -> Result<()> {
        match self {
            StorageOrRemoteDB::Remote(remote_db) => {
                remote_db.store_block(block_height, block).await?
            }
            StorageOrRemoteDB::Storage(storage_db) => {
                storage_db.store_block(block_height, block).await?
            }
        }
        Ok(())
    }
}

impl<S> BlocksProvider for StorageOrRemoteBlocksProvider<S>
where
    S: 'static,
    S: KeyValueInspect<Column = Column>,
    S: AtomicView,
    S::LatestView: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static,
{
    type Block = Arc<Vec<u8>>;
    type BlockRangeResponse = BlockRangeResponse;

    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<Self::BlockRangeResponse> {
        let range_response = match self {
            StorageOrRemoteBlocksProvider::Remote(remote_db) => {
                remote_db.get_block_range(first, last)?
            }
            StorageOrRemoteBlocksProvider::Storage(storage_db) => {
                storage_db.get_block_range(first, last)?
            }
        };
        Ok(range_response)
    }

    fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let height = match self {
            StorageOrRemoteBlocksProvider::Remote(remote_db) => {
                remote_db.get_current_height()?
            }
            StorageOrRemoteBlocksProvider::Storage(storage_db) => {
                storage_db.get_current_height()?
            }
        };
        Ok(height)
    }
}
