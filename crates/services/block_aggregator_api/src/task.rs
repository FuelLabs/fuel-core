use crate::{
    blocks::{
        BlockSource,
        BlockSourceEvent,
    },
    db::{
        BlocksProvider,
        BlocksStorage,
    },
    service::SharedState,
};
use fuel_core_services::{
    RunnableTask,
    Service,
    StateWatcher,
    TaskNextAction,
    try_or_stop,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

/// The Block Aggregator service, which aggregates blocks from a source and stores them in a database
/// Queries can be made to the service to retrieve data from the `DB`
pub struct Task<S1, S2, Blocks>
where
    S2: BlocksProvider,
{
    pub(crate) api: Box<dyn Service + Send + Sync + 'static>,
    pub(crate) storage: S1,
    pub(crate) shared_state: SharedState<S2>,
    pub(crate) block_source: Blocks,
}

impl<S1, S2, Blocks> Task<S1, S2, Blocks>
where
    S1: BlocksStorage<Block = Blocks::Block>,
    S2: BlocksProvider<Block = Blocks::Block>,
    Blocks: BlockSource,
    <Blocks as BlockSource>::Block: Send + Sync + Debug,
{
    pub fn new(
        api: Box<dyn Service + Send + Sync + 'static>,
        storage: S1,
        shared_state: SharedState<S2>,
        block_source: Blocks,
    ) -> Self {
        Self {
            api,
            storage,
            shared_state,
            block_source,
        }
    }

    pub async fn handle_block(
        &mut self,
        res: crate::result::Result<BlockSourceEvent<<Blocks as BlockSource>::Block>>,
    ) -> TaskNextAction {
        tracing::debug!("Handling block: {res:?}");
        let event = try_or_stop!(res, |e| {
            tracing::error!("Error receiving block from source: {e:?}");
        });
        let res = self.storage.store_block(&event).await;
        try_or_stop!(res, |e| {
            tracing::error!("Error storing block in database: {e:?}");
        });

        match event {
            BlockSourceEvent::NewBlock(height, block) => {
                let _ = self
                    .shared_state
                    .blocks_broadcast
                    .send((height, Arc::new(block)));
            }
            BlockSourceEvent::OldBlock(_id, _block) => {
                // Do nothing
                // Only stream new blocks
            }
        };
        TaskNextAction::Continue
    }
}

impl<S1, S2, Blocks> RunnableTask for Task<S1, S2, Blocks>
where
    S1: BlocksStorage<Block = Blocks::Block>,
    S2: BlocksProvider<Block = Blocks::Block>,
    Blocks: BlockSource,
    <Blocks as BlockSource>::Block: Send + Sync + Debug,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            block_res = self.block_source.next_block() => self.handle_block(block_res).await,
            _ = self.api.await_stop() => TaskNextAction::Stop,
            _ = watcher.while_started() => TaskNextAction::Stop,
        }
    }

    async fn shutdown(mut self) -> anyhow::Result<()> {
        self.block_source.drain().await.map_err(|e| {
            anyhow::anyhow!("Error draining block source during shutdown: {e:?}")
        })?;
        self.api.stop_and_await().await?;
        Ok(())
    }
}
