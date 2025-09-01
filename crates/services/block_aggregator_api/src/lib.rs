use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    try_or_stop,
};

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
use result::Result;

pub mod api;
pub mod blocks;
pub mod db;
pub mod result;

#[cfg(test)]
mod tests;

// TODO: this doesn't need to limited to the blocks,
//   but we can change the name later
pub struct BlockAggregator<Api, DB, Blocks> {
    query: Api,
    database: DB,
    _block_source: Blocks,
}

impl<Api, DB, Blocks> RunnableTask for BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi,
    DB: BlockAggregatorDB,
    Blocks: BlockSource,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            query_res = self.query.await_query() => self.handle_query(query_res),
            block_res = self._block_source.next_block() => self.handle_block(block_res),
            _ = watcher.while_started() => self.stop(),
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<Api, DB, Blocks> BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi,
    DB: BlockAggregatorDB,
    Blocks: BlockSource,
{
    pub fn new(query: Api, database: DB, _block_source: Blocks) -> Self {
        Self {
            query,
            database,
            _block_source,
        }
    }

    pub fn stop(&self) -> TaskNextAction {
        TaskNextAction::Stop
    }

    pub fn handle_query(&mut self, res: Result<BlockAggregatorQuery>) -> TaskNextAction {
        let query = try_or_stop!(res, |e| {
            tracing::error!("Error receiving query: {e:?}");
        });
        match query {
            BlockAggregatorQuery::GetBlockRange {
                first,
                last,
                response,
            } => {
                let res = self.database.get_block_range(first, last);
                let block_stream = try_or_stop!(res, |e| {
                    tracing::error!("Error getting block range from database: {e:?}");
                });
                let res = response.send(block_stream);
                try_or_stop!(res, |_| {
                    tracing::error!("Error sending block range response");
                });
                TaskNextAction::Continue
            }
        }
    }

    pub fn handle_block(&mut self, _res: Result<Block>) -> TaskNextAction {
        todo!()
    }
}
