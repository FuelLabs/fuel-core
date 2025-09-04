use crate::{
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
    },
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
    try_or_stop,
};
use result::Result;

pub mod api;
pub mod blocks;
pub mod db;
pub mod result;

pub mod block_range_response;

#[cfg(test)]
mod tests;

// TODO: this doesn't need to limited to the blocks,
//   but we can change the name later
pub struct BlockAggregator<Api, DB, Blocks> {
    query: Api,
    database: DB,
    block_source: Blocks,
}

impl<Api, DB, Blocks, BlockRange> RunnableTask for BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi<BlockRangeResponse = BlockRange>,
    DB: BlockAggregatorDB<BlockRange = BlockRange>,
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

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<Api, DB, Blocks, BlockRange> BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi<BlockRangeResponse = BlockRange>,
    DB: BlockAggregatorDB<BlockRange = BlockRange>,
    Blocks: BlockSource,
    BlockRange: Send,
{
    pub fn new(query: Api, database: DB, block_source: Blocks) -> Self {
        Self {
            query,
            database,
            block_source,
        }
    }

    pub fn stop(&self) -> TaskNextAction {
        TaskNextAction::Stop
    }

    pub async fn handle_query(
        &mut self,
        res: Result<BlockAggregatorQuery<BlockRange>>,
    ) -> TaskNextAction {
        tracing::debug!("Handling query: {res:?}");
        let query = try_or_stop!(res, |e| {
            tracing::error!("Error receiving query: {e:?}");
        });
        match query {
            BlockAggregatorQuery::GetBlockRange {
                first,
                last,
                response,
            } => {
                self.handle_get_block_range_query(first, last, response)
                    .await
            }
            BlockAggregatorQuery::GetCurrentHeight { response } => {
                self.handle_get_current_height_query(response).await
            }
        }
    }

    async fn handle_get_block_range_query(
        &mut self,
        first: u64,
        last: u64,
        response: tokio::sync::oneshot::Sender<BlockRange>,
    ) -> TaskNextAction {
        let res = self.database.get_block_range(first, last).await;
        let block_stream = try_or_stop!(res, |e| {
            tracing::error!("Error getting block range from database: {e:?}");
        });
        let res = response.send(block_stream);
        try_or_stop!(res, |_| {
            tracing::error!("Error sending block range response");
        });
        TaskNextAction::Continue
    }

    async fn handle_get_current_height_query(
        &mut self,
        response: tokio::sync::oneshot::Sender<u64>,
    ) -> TaskNextAction {
        let res = self.database.get_current_height().await;
        let height = try_or_stop!(res, |e| {
            tracing::error!("Error getting current height from database: {e:?}");
        });
        let res = response.send(height);
        try_or_stop!(res, |_| {
            tracing::error!("Error sending current height response");
        });
        TaskNextAction::Continue
    }

    pub async fn handle_block(&mut self, res: Result<(u64, Block)>) -> TaskNextAction {
        tracing::debug!("Handling block: {res:?}");
        let (id, block) = try_or_stop!(res, |e| {
            tracing::error!("Error receiving block from source: {e:?}");
        });
        let res = self.database.store_block(id, block).await;
        try_or_stop!(res, |e| {
            tracing::error!("Error storing block in database: {e:?}");
        });
        TaskNextAction::Continue
    }
}
