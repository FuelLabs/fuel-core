use crate::{api::BlockAggregatorApi, blocks::BlockSource, db::BlockAggregatorDB};
use fuel_core_services::{RunnableService, RunnableTask, StateWatcher, TaskNextAction};
use fuel_core_types::fuel_types::BlockHeight;
use protobuf_types::Block as ProtoBlock;
use std::fmt::Debug;

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
        api::{BlockAggregatorApi, protobuf_adapter::ProtobufAPI},
        blocks::importer_and_db_source::{BlockSerializer, ImporterAndDbSource},
        db::BlockAggregatorDB,
        protobuf_types::Block as ProtoBlock,
    };
    use fuel_core_services::{ServiceRunner, stream::BoxStream};
    use fuel_core_storage::{
        StorageInspect,
        tables::{FuelBlocks, Transactions},
        transactional::HistoricalView,
    };
    use fuel_core_types::{
        fuel_types::BlockHeight, services::block_importer::SharedImportResult,
    };
    use std::net::SocketAddr;
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

    #[allow(clippy::type_complexity)]
    pub fn new_service<DB, S, OnchainDB, E>(
        config: &Config,
        db: DB,
        serializer: S,
        onchain_db: OnchainDB,
        importer: BoxStream<SharedImportResult>,
        sync_from_height: BlockHeight,
    ) -> anyhow::Result<ServiceRunner<
        BlockAggregator<
            ProtobufAPI,
            DB,
            ImporterAndDbSource<S, OnchainDB, E>,
            ProtoBlock,
        >,
    >>
    where
        DB: BlockAggregatorDB<
            BlockRangeResponse = <ProtobufAPI as BlockAggregatorApi>::BlockRangeResponse,
            Block = ProtoBlock,
        >,
        S: BlockSerializer<Block=ProtoBlock> + Clone + Send + Sync + 'static,
        OnchainDB: Send + Sync,
        OnchainDB: StorageInspect<FuelBlocks, Error = E>,
        OnchainDB: StorageInspect<Transactions, Error = E>,
        OnchainDB: HistoricalView<Height = BlockHeight>,
        E: std::fmt::Debug + Send + Sync,
    {
        let addr = config.addr.to_string();
        let api = ProtobufAPI::new(addr)
            .map_err(|e| anyhow::anyhow!("Error creating API: {e}"))?;
        let db_ending_height = onchain_db
            .latest_height()
            .and_then(BlockHeight::succ)
            .unwrap_or(BlockHeight::from(0));
        let block_source = ImporterAndDbSource::new(
            importer,
            serializer,
            onchain_db,
            sync_from_height,
            db_ending_height,
        );
        let block_aggregator = BlockAggregator {
            query: api,
            database: db,
            block_source,
            new_block_subscriptions: Vec::new(),
        };
        let runner = ServiceRunner::new(block_aggregator);
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

#[async_trait::async_trait]
impl<Api, DB, Blocks, BlockRange> RunnableService
    for BlockAggregator<Api, DB, Blocks, Blocks::Block>
where
    Api:
        BlockAggregatorApi<Block = Blocks::Block, BlockRangeResponse = BlockRange> + Send,
    DB: BlockAggregatorDB<Block = Blocks::Block, BlockRangeResponse = BlockRange> + Send,
    Blocks: BlockSource,
    BlockRange: Send,
    <Blocks as BlockSource>::Block: Clone + Debug + Send,
{
    const NAME: &'static str = "BlockAggregatorService";
    type SharedData = ();
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}
