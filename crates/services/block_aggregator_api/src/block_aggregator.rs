use crate::{
    BlockAggregator,
    NewBlock,
    api::{
        BlockAggregatorApi,
        BlockAggregatorQuery,
    },
    blocks::{
        BlockSource,
        BlockSourceEvent,
    },
    db::BlockAggregatorDB,
};
use fuel_core_services::{
    TaskNextAction,
    try_or_stop,
};
use fuel_core_types::fuel_types::BlockHeight;

impl<Api, DB, Blocks, BlockRangeResponse> BlockAggregator<Api, DB, Blocks>
where
    Api: BlockAggregatorApi<BlockRangeResponse = BlockRangeResponse>,
    DB: BlockAggregatorDB<BlockRangeResponse = BlockRangeResponse>,
    Blocks: BlockSource,
    BlockRangeResponse: Send,
{
    pub fn new(query: Api, database: DB, block_source: Blocks) -> Self {
        let new_block_subscriptions = Vec::new();
        Self {
            query,
            database,
            block_source,
            new_block_subscriptions,
        }
    }

    pub fn stop(&self) -> TaskNextAction {
        TaskNextAction::Stop
    }

    pub async fn handle_query(
        &mut self,
        res: crate::result::Result<BlockAggregatorQuery<BlockRangeResponse>>,
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
            BlockAggregatorQuery::NewBlockSubscription { response } => {
                self.handle_new_block_subscription(response).await
            }
        }
    }

    async fn handle_get_block_range_query(
        &mut self,
        first: BlockHeight,
        last: BlockHeight,
        response: tokio::sync::oneshot::Sender<BlockRangeResponse>,
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
        response: tokio::sync::oneshot::Sender<BlockHeight>,
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

    async fn handle_new_block_subscription(
        &mut self,
        response: tokio::sync::mpsc::Sender<NewBlock>,
    ) -> TaskNextAction {
        self.new_block_subscriptions.push(response);
        TaskNextAction::Continue
    }

    pub async fn handle_block(
        &mut self,
        res: crate::result::Result<BlockSourceEvent>,
    ) -> TaskNextAction {
        tracing::debug!("Handling block: {res:?}");
        let event = try_or_stop!(res, |e| {
            tracing::error!("Error receiving block from source: {e:?}");
        });
        let (id, block) = match event {
            BlockSourceEvent::NewBlock(id, block) => {
                self.new_block_subscriptions.retain_mut(|sub| {
                    let send_res = sub.try_send(NewBlock::new(id, block.clone()));
                    match send_res {
                        Ok(_) => true,
                        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                            tracing::error!("Error sending new block to subscriber due to full channel: {id:?}");
                            true
                        },
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                            tracing::debug!("Dropping block subscription due to closed channel");
                            false
                        },
                    }
                });
                (id, block)
            }
            BlockSourceEvent::OldBlock(id, block) => (id, block),
        };
        let res = self.database.store_block(id, block).await;
        try_or_stop!(res, |e| {
            tracing::error!("Error storing block in database: {e:?}");
        });
        TaskNextAction::Continue
    }
}
