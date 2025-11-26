use crate::{
    api::BlockAggregatorApi,
    blocks::BlockSource,
    db::BlockAggregatorDB,
};
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::fuel_types::BlockHeight;
use protobuf_types::Block as ProtoBlock;

pub mod api;
pub mod blocks;
pub mod db;
pub mod result;

pub mod block_range_response;

pub mod block_aggregator;
pub mod protobuf_types;

#[cfg(test)]
mod tests;

pub mod integration {
    use crate::{
        BlockAggregator,
        api::{
            BlockAggregatorApi,
            protobuf_adapter::ProtobufAPI,
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
            storage_or_remote_db::StorageOrRemoteDB,
            table::{
                Column,
                LatestBlock,
                Mode,
            },
        },
        protobuf_types::Block as ProtoBlock,
    };
    use anyhow::bail;
    use fuel_core_services::{
        RunnableService,
        ServiceRunner,
        StateWatcher,
        stream::BoxStream,
    };
    use fuel_core_storage::{
        Error as StorageError,
        StorageAsRef,
        StorageInspect,
        StorageMutate,
        kv_store::KeyValueInspect,
        tables::{
            FuelBlocks,
            Transactions,
        },
        transactional::{
            AtomicView,
            HistoricalView,
            Modifiable,
            StorageTransaction,
        },
    };
    use fuel_core_types::{
        fuel_types::BlockHeight,
        services::block_importer::SharedImportResult,
    };
    use std::{
        fmt::Debug,
        net::SocketAddr,
    };

    #[derive(Clone, Debug)]
    pub struct Config {
        pub addr: SocketAddr,
        pub sync_from: Option<BlockHeight>,
        pub storage_method: StorageMethod,
    }

    #[derive(Clone, Debug, Default)]
    pub enum StorageMethod {
        #[default]
        Local,
        S3 {
            bucket: String,
            endpoint_url: Option<String>,
            requester_pays: bool,
        },
    }

    pub struct UninitializedTask<API, Blocks, S> {
        api: API,
        block_source: Blocks,
        storage: S,
        config: Config,
        genesis_block_height: BlockHeight,
    }

    #[async_trait::async_trait]
    impl<Api, Blocks, S, T> RunnableService for UninitializedTask<Api, Blocks, S>
    where
        Api: BlockAggregatorApi<
                Block = ProtoBlock,
                BlockRangeResponse = BlockRangeResponse,
            >,
        Blocks: BlockSource<Block = ProtoBlock>,
        // Storage Constraints
        S: Modifiable + Debug,
        S: KeyValueInspect<Column = Column>,
        S: StorageInspect<LatestBlock, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut S>:
            StorageMutate<crate::db::table::Blocks, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut S>:
            StorageMutate<LatestBlock, Error = StorageError>,
        S: AtomicView<LatestView = T>,
        T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + Debug,
        StorageTransaction<T>:
            StorageInspect<crate::db::table::Blocks, Error = StorageError>,
        // Remote Constraints
        S: Send + Sync,
        S: Modifiable,
        S: StorageInspect<LatestBlock, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut S>:
            StorageMutate<LatestBlock, Error = StorageError>,
    {
        const NAME: &'static str = "BlockAggregatorService";
        type SharedData = ();
        type Task = BlockAggregator<Api, StorageOrRemoteDB<S>, Blocks, Blocks::Block>;
        type TaskParams = ();

        fn shared_data(&self) -> Self::SharedData {}

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
            } = self;
            let sync_from = config.sync_from.unwrap_or(genesis_block_height);
            let db_adapter = match config.storage_method {
                StorageMethod::Local => {
                    let mode = storage.storage_as_ref::<LatestBlock>().get(&())?;
                    let maybe_sync_from_height = match mode
                        .clone()
                        .map(|c| c.into_owned())
                    {
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
                    requester_pays,
                } => {
                    let mode = storage.storage_as_ref::<LatestBlock>().get(&())?;
                    let maybe_sync_from_height = match mode
                        .clone()
                        .map(|c| c.into_owned())
                    {
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
                        &bucket,
                        requester_pays,
                        endpoint_url.clone(),
                        sync_from_height,
                    )
                    .await
                }
            };
            Ok(BlockAggregator {
                query: api,
                database: db_adapter,
                block_source,
                new_block_subscriptions: vec![],
            })
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
        ServiceRunner<
            UninitializedTask<
                ProtobufAPI,
                ImporterAndDbSource<S, OnchainDB, Receipts>,
                DB,
            >,
        >,
    >
    where
        S: BlockSerializer<Block = ProtoBlock> + Clone + Send + Sync + 'static,
        OnchainDB: Send + Sync,
        OnchainDB: StorageInspect<FuelBlocks, Error = StorageError>,
        OnchainDB: StorageInspect<Transactions, Error = StorageError>,
        OnchainDB: HistoricalView<Height = BlockHeight>,
        Receipts: TxReceipts,
        // Storage Constraints
        DB: Modifiable + Debug,
        DB: KeyValueInspect<Column = Column>,
        DB: StorageInspect<LatestBlock, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut DB>:
            StorageMutate<crate::db::table::Blocks, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut DB>:
            StorageMutate<LatestBlock, Error = StorageError>,
        DB: AtomicView<LatestView = T>,
        T: Unpin + Send + Sync + KeyValueInspect<Column = Column> + 'static + Debug,
        StorageTransaction<T>:
            StorageInspect<crate::db::table::Blocks, Error = StorageError>,
        // Remote Constraints
        DB: Send + Sync,
        DB: Modifiable,
        DB: StorageInspect<LatestBlock, Error = StorageError>,
        for<'b> StorageTransaction<&'b mut DB>:
            StorageMutate<LatestBlock, Error = StorageError>,
    {
        let addr = config.addr.to_string();
        let api = ProtobufAPI::new(addr)
            .map_err(|e| anyhow::anyhow!("Error creating API: {e}"))?;
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
            api,
            block_source,
            storage: db,
            config,
            genesis_block_height,
        };
        let runner = ServiceRunner::new(uninitialized_task);
        Ok(runner)
    }
}

// TODO: this doesn't need to limited to the blocks,
//   but we can change the name later
/// The Block Aggregator service, which aggregates blocks from a source and stores them in a database
/// Queries can be made to the service to retrieve data from the `DB`
pub struct BlockAggregator<Api, DB, Blocks, Block> {
    query: Api,
    database: DB,
    block_source: Blocks,
    new_block_subscriptions: Vec<tokio::sync::mpsc::Sender<(BlockHeight, Block)>>,
}

pub struct NewBlock {
    height: BlockHeight,
    block: ProtoBlock,
}

impl NewBlock {
    pub fn new(height: BlockHeight, block: ProtoBlock) -> Self {
        Self { height, block }
    }

    pub fn into_inner(self) -> (BlockHeight, ProtoBlock) {
        (self.height, self.block)
    }
}

impl<Api, DB, Blocks, BlockRange> RunnableTask
    for BlockAggregator<Api, DB, Blocks, Blocks::Block>
where
    Api: BlockAggregatorApi<Block = Blocks::Block, BlockRangeResponse = BlockRange>,
    DB: BlockAggregatorDB<Block = Blocks::Block, BlockRangeResponse = BlockRange>,
    Blocks: BlockSource,
    <Blocks as BlockSource>::Block: Clone + std::fmt::Debug + Send,
    BlockRange: Send,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tracing::debug!("BlockAggregator running");
        tokio::select! {
            query_res = self.query.await_query() => self.handle_query(query_res).await,
            block_res = self.block_source.next_block() => self.handle_block(block_res).await,
            _ = watcher.while_started() => self.stop(),
        }
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        self.block_source.drain().await.map_err(|e| {
            anyhow::anyhow!("Error draining block source during shutdown: {e:?}")
        })?;
        Ok(())
    }
}

// #[async_trait::async_trait]
// impl<Api, DB, Blocks, BlockRange> RunnableService
//     for BlockAggregator<Api, DB, Blocks, Blocks::Block>
// where
//     Api:
//         BlockAggregatorApi<Block = Blocks::Block, BlockRangeResponse = BlockRange> + Send,
//     DB: BlockAggregatorDB<Block = Blocks::Block, BlockRangeResponse = BlockRange> + Send,
//     Blocks: BlockSource,
//     BlockRange: Send,
//     <Blocks as BlockSource>::Block: Clone + Debug + Send,
// {
//     const NAME: &'static str = "BlockAggregatorService";
//     type SharedData = ();
//     type Task = Self;
//     type TaskParams = ();
//
//     fn shared_data(&self) -> Self::SharedData {}
//
//     async fn into_task(
//         self,
//         _state_watcher: &StateWatcher,
//         _params: Self::TaskParams,
//     ) -> anyhow::Result<Self::Task> {
//         Ok(self)
//     }
// }
