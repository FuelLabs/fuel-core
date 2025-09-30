use crate::{
    api::BlockAggregatorApi,
    blocks::{
        Block,
        BlockSource,
    },
    db::BlockStorage,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::fuel_types::BlockHeight;

pub mod api;
pub mod blocks;
pub mod db;
pub mod result;

pub mod block_range_response;

pub mod integration {
    use crate::{
        BlockAggregator,
        api::{
            BlockAggregatorApi,
            protobuf_adapter::ProtobufAPI,
        },
        blocks::importer_and_db_source::{
            BlockSerializer,
            ImporterAndDbSource,
        },
        db::BlockStorage,
    };
    use fuel_core_services::{
        ServiceRunner,
        stream::BoxStream,
    };
    use fuel_core_storage::{
        StorageInspect,
        tables::{
            FuelBlocks,
            Transactions,
        },
    };
    use fuel_core_types::{
        fuel_types::BlockHeight,
        services::block_importer::SharedImportResult,
    };
    use std::net::SocketAddr;

    #[derive(Clone, Debug)]
    pub struct Config {
        pub addr: SocketAddr,
    }

    pub fn new_service<DB, S, OnchainDB, E>(
        config: &Config,
        db: DB,
        serializer: S,
        onchain_db: OnchainDB,
        importer: BoxStream<SharedImportResult>,
    ) -> ServiceRunner<
        BlockAggregator<ProtobufAPI, DB, ImporterAndDbSource<S, OnchainDB, E>>,
    >
    where
        DB: BlockStorage<
            BlockRangeResponse = <ProtobufAPI as BlockAggregatorApi>::BlockRangeResponse,
        >,
        S: BlockSerializer + Clone + Send + Sync + 'static,
        OnchainDB: Send + Sync,
        OnchainDB: StorageInspect<FuelBlocks, Error = E>,
        OnchainDB: StorageInspect<Transactions, Error = E>,
        E: std::fmt::Debug + Send + Sync,
    {
        let addr = config.addr.to_string();
        let api = ProtobufAPI::new(addr);
        let db_starting_height = BlockHeight::from(0);
        let db_ending_height = None;
        let block_source = ImporterAndDbSource::new(
            importer,
            serializer,
            onchain_db,
            db_starting_height,
            db_ending_height,
        );
        let block_aggregator = BlockAggregator {
            query: api,
            database: db,
            block_source,
            new_block_subscriptions: Vec::new(),
        };
        ServiceRunner::new(block_aggregator)
    }
}
#[cfg(test)]
mod tests;

pub mod block_aggregator;

// TODO: this doesn't need to limited to the blocks,
//   but we can change the name later
/// The Block Aggregator service, which aggregates blocks from a source and stores them in a database
/// Queries can be made to the service to retrieve data from the `DB`
pub struct BlockAggregator<Api, DB, Blocks> {
    query: Api,
    database: DB,
    block_source: Blocks,
    new_block_subscriptions: Vec<tokio::sync::mpsc::Sender<NewBlock>>,
}

pub struct NewBlock {
    height: BlockHeight,
    block: Block,
}

impl NewBlock {
    pub fn new(height: BlockHeight, block: Block) -> Self {
        Self { height, block }
    }

    pub fn into_inner(self) -> (BlockHeight, Block) {
        (self.height, self.block)
    }
}

impl<Api, DB, Blocks, BlockRange> RunnableTask for BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi<BlockRangeResponse = BlockRange>,
    DB: BlockStorage<BlockRangeResponse = BlockRange>,
    Blocks: BlockSource,
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
impl<Api, DB, Blocks, BlockRange> RunnableService for BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi<BlockRangeResponse = BlockRange>,
    DB: BlockStorage<BlockRangeResponse = BlockRange>,
    Blocks: BlockSource,
    BlockRange: Send,
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
