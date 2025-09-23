use crate::{
    api::BlockAggregatorApi,
    blocks::{
        Block,
        BlockSource,
    },
    db::BlockAggregatorDB,
};
use fuel_core_services::{
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
    DB: BlockAggregatorDB<BlockRangeResponse = BlockRange>,
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
