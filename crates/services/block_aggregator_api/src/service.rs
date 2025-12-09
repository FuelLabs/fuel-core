use crate::{
    api::{
        protobuf_adapter,
        protobuf_adapter::BlocksAggregatorApi,
    },
    block_range_response::BlockRangeResponse,
    blocks::{
        BlockSource,
        importer_and_db_source::{
            BlockSerializer,
            ImporterAndDbSource,
            sync_service::TxReceipts,
        },
    },
    db::{
        BlocksProvider,
        storage_or_remote_db::{
            StorageOrRemoteBlocksProvider,
            StorageOrRemoteDB,
        },
        table::{
            Column,
            LatestBlock,
            Mode,
        },
    },
    protobuf_types::Block as ProtoBlock,
    result::Result as BlockAggregatorResult,
    task::Task,
};
use anyhow::bail;
use fuel_core_services::{
    RunnableService,
    Service,
    ServiceRunner,
    StateWatcher,
    stream::BoxStream,
};
use fuel_core_storage::{
    Error as StorageError,
    StorageAsRef,
    StorageInspect,
    kv_store::KeyValueInspect,
    structured_storage::AsStructuredStorage,
    tables::{
        FuelBlocks,
        Transactions,
    },
    transactional::{
        AtomicView,
        HistoricalView,
        Modifiable,
    },
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use futures::Stream;
use std::{
    fmt::Debug,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub api_buffer_size: usize,
    pub sync_from: Option<BlockHeight>,
    pub storage_method: StorageMethod,
}

#[derive(Clone, Debug, Default)]
pub enum StorageMethod {
    // Stores blocks in local DB
    #[default]
    Local,
    // Publishes blocks to S3 bucket
    S3 {
        bucket: String,
        endpoint_url: Option<String>,
        requester_pays: bool,
    },
    // Assumes another node is publishing blocks to S3 bucket, but relaying details
    S3NoPublish {
        bucket: String,
        endpoint_url: Option<String>,
        requester_pays: bool,
    },
}

pub struct SharedState<S>
where
    S: BlocksProvider,
{
    pub(crate) storage: Arc<S>,
    pub(crate) blocks_broadcast: broadcast::Sender<(BlockHeight, Arc<S::Block>)>,
}

impl<S> SharedState<S>
where
    S: BlocksProvider,
{
    pub fn new(storage: S, channel_size: usize) -> Self {
        let (blocks_broadcast, _) = broadcast::channel(channel_size);
        SharedState {
            storage: Arc::new(storage),
            blocks_broadcast,
        }
    }
}

impl<S> Clone for SharedState<S>
where
    S: BlocksProvider,
{
    fn clone(&self) -> Self {
        SharedState {
            storage: Arc::clone(&self.storage),
            blocks_broadcast: self.blocks_broadcast.clone(),
        }
    }
}

impl<S> SharedState<S>
where
    S: BlocksProvider,
{
    fn storage(&self) -> &S {
        &self.storage
    }

    pub fn get_block_range<H>(
        &self,
        first: H,
        last: H,
    ) -> BlockAggregatorResult<S::BlockRangeResponse>
    where
        H: Into<BlockHeight>,
    {
        self.storage().get_block_range(first.into(), last.into())
    }

    pub fn get_current_height(&self) -> BlockAggregatorResult<Option<BlockHeight>> {
        self.storage().get_current_height()
    }

    pub fn new_block_subscription(
        &self,
    ) -> impl Stream<Item = anyhow::Result<(BlockHeight, Arc<S::Block>)>> + 'static {
        let receiver = self.blocks_broadcast.subscribe();
        tokio_stream::wrappers::BroadcastStream::new(receiver)
            .map(|result| result.map_err(|e| anyhow::anyhow!("Broadcast error: {:?}", e)))
    }
}

impl<S> BlocksAggregatorApi for SharedState<S>
where
    S: BlocksProvider<Block = ProtoBlock, BlockRangeResponse = BlockRangeResponse>,
{
    fn get_block_range<H: Into<BlockHeight>>(
        &self,
        first: H,
        last: H,
    ) -> BlockAggregatorResult<S::BlockRangeResponse> {
        self.get_block_range(first, last)
    }

    fn get_current_height(&self) -> BlockAggregatorResult<Option<BlockHeight>> {
        self.get_current_height()
    }

    fn new_block_subscription(
        &self,
    ) -> impl Stream<Item = anyhow::Result<(BlockHeight, Arc<S::Block>)>> + Send + 'static
    {
        self.new_block_subscription()
    }
}

pub struct UninitializedTask<Blocks, S1, S2>
where
    S2: KeyValueInspect<Column = Column> + 'static,
    S2: AtomicView,
    S2::LatestView: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static,
{
    api: Box<dyn Service + Send + Sync + 'static>,
    block_source: Blocks,
    storage: S1,
    config: Config,
    genesis_block_height: BlockHeight,
    shared_state: SharedState<StorageOrRemoteBlocksProvider<S2>>,
}

#[async_trait::async_trait]
impl<Blocks, S1, S2> RunnableService for UninitializedTask<Blocks, S1, S2>
where
    Blocks: BlockSource<Block = ProtoBlock>,
    S1: Send + Sync + Modifiable + Debug + 'static,
    S1: KeyValueInspect<Column = Column>,
    S2: KeyValueInspect<Column = Column> + 'static,
    S2: AtomicView,
    S2::LatestView: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static,
{
    const NAME: &'static str = "BlockAggregatorService";
    type SharedData = SharedState<StorageOrRemoteBlocksProvider<S2>>;
    type Task = Task<StorageOrRemoteDB<S1>, StorageOrRemoteBlocksProvider<S2>, Blocks>;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        let UninitializedTask {
            api,
            block_source,
            storage,
            config,
            genesis_block_height,
            shared_state,
        } = self;
        let sync_from = config.sync_from.unwrap_or(genesis_block_height);

        let publish = matches!(config.storage_method, StorageMethod::S3 { .. });

        let db_adapter = match config.storage_method {
            StorageMethod::Local => {
                let mode = storage
                    .as_structured_storage()
                    .storage_as_ref::<LatestBlock>()
                    .get(&())?
                    .map(|c| c.into_owned());
                let maybe_sync_from_height = match mode {
                    Some(Mode::S3(_)) => {
                        bail!(
                            "Database is configured in S3 mode, but Local storage method was requested. If you would like to run in S3 mode, then please use a clean DB"
                        );
                    }
                    _ => mode.map(|m| m.height()),
                };
                let sync_from_height = maybe_sync_from_height.unwrap_or(sync_from);
                StorageOrRemoteDB::new_storage(storage, sync_from_height)
            }
            StorageMethod::S3 {
                bucket,
                endpoint_url,
                ..
            }
            | StorageMethod::S3NoPublish {
                bucket,
                endpoint_url,
                ..
            } => {
                let mode = storage
                    .as_structured_storage()
                    .storage_as_ref::<LatestBlock>()
                    .get(&())?
                    .map(|c| c.into_owned());
                let maybe_sync_from_height = match mode {
                    Some(Mode::Local(_)) => {
                        bail!(
                            "Database is configured in S3 mode, but Local storage method was requested. If you would like to run in S3 mode, then please use a clean DB"
                        );
                    }
                    _ => mode.map(|m| m.height()),
                };
                let sync_from_height = maybe_sync_from_height.unwrap_or(sync_from);

                StorageOrRemoteDB::new_s3(
                    storage,
                    bucket,
                    endpoint_url.clone(),
                    sync_from_height,
                    publish,
                )
                .await
            }
        };

        api.start_and_await().await?;

        Ok(Task::new(api, db_adapter, shared_state, block_source))
    }
}

#[allow(clippy::type_complexity)]
pub fn new_service<DB, S, OnchainDB, Receipts, T>(
    db: DB,
    serializer: S,
    onchain_db: OnchainDB,
    receipts: Receipts,
    importer: BoxStream<SharedImportResult>,
    config: Config,
    genesis_block_height: BlockHeight,
) -> anyhow::Result<
    ServiceRunner<UninitializedTask<ImporterAndDbSource<S, OnchainDB, Receipts>, DB, DB>>,
>
where
    S: BlockSerializer<Block = ProtoBlock> + Clone + Send + Sync + 'static,
    OnchainDB: Send + Sync,
    OnchainDB: StorageInspect<FuelBlocks, Error = StorageError>,
    OnchainDB: StorageInspect<Transactions, Error = StorageError>,
    OnchainDB: HistoricalView<Height = BlockHeight>,
    Receipts: TxReceipts,
    // Storage Constraints
    DB: Modifiable + Debug + Clone + Send + Sync + 'static,
    DB: KeyValueInspect<Column = Column>,
    DB: AtomicView<LatestView = T>,
    T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + Debug,
{
    let db_adapter = match &config.storage_method {
        StorageMethod::Local => StorageOrRemoteBlocksProvider::new_storage(db.clone()),
        StorageMethod::S3 {
            bucket,
            endpoint_url,
            requester_pays,
        }
        | StorageMethod::S3NoPublish {
            bucket,
            endpoint_url,
            requester_pays,
        } => StorageOrRemoteBlocksProvider::new_s3(
            db.clone(),
            bucket.clone(),
            *requester_pays,
            endpoint_url.clone(),
        ),
    };
    let shared_state = SharedState::new(db_adapter, config.api_buffer_size);

    let api = protobuf_adapter::new_service(config.addr, shared_state.clone());

    let db_ending_height = onchain_db
        .latest_height()
        .and_then(BlockHeight::succ)
        .unwrap_or(BlockHeight::from(0));

    let sync_from_height = config.sync_from.unwrap_or(genesis_block_height);
    let block_source = ImporterAndDbSource::new(
        importer,
        serializer,
        onchain_db,
        receipts,
        sync_from_height,
        db_ending_height,
    );

    let uninitialized_task = UninitializedTask {
        api: Box::new(api),
        block_source,
        shared_state,
        config,
        storage: db,
        genesis_block_height,
    };

    let runner = ServiceRunner::new(uninitialized_task);
    Ok(runner)
}
