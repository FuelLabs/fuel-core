use crate::{
    blocks::BlockSource,
    db::{
        BlocksProvider,
        BlocksStorage,
    },
    result::{
        Error,
        Result as AggregatorResult,
    },
    service::SharedState,
};
use fuel_core_services::{
    RunnableTask,
    Service,
    StateWatcher,
    TaskNextAction,
    stream::{
        BoxStream,
        IntoBoxStream,
    },
    try_or_stop,
};
use fuel_core_types::fuel_types::BlockHeight;
use futures::StreamExt;
use std::fmt::Debug;

/// The Block Aggregator service, which aggregates blocks from a source and stores them in a database
/// Queries can be made to the service to retrieve data from the `DB`
pub struct Task<S1, S2, Blocks>
where
    S2: BlocksProvider,
    Blocks: BlockSource,
{
    pub(crate) sync_from: BlockHeight,
    pub(crate) api: Box<dyn Service + Send + Sync + 'static>,
    pub(crate) storage: S1,
    pub(crate) shared_state: SharedState<S2>,
    pub(crate) block_source: Blocks,
    pub(crate) importer:
        BoxStream<anyhow::Result<(BlockHeight, <Blocks as BlockSource>::Block)>>,
    pub(crate) last_seen_importer_height: Option<BlockHeight>,
    /// A joint stream of old blocks from the block source and new blocks from the importer
    pub(crate) old_and_new_block_stream:
        BoxStream<AggregatorResult<(BlockHeight, <Blocks as BlockSource>::Block)>>,
}

impl<S1, S2, Blocks> Task<S1, S2, Blocks>
where
    S1: BlocksStorage<Block = Blocks::Block>,
    S2: BlocksProvider<Block = Blocks::Block>,
    Blocks: BlockSource,
    <Blocks as BlockSource>::Block: Clone + Send + Sync + Debug + 'static,
{
    pub fn new(
        sync_from: BlockHeight,
        api: Box<dyn Service + Send + Sync + 'static>,
        storage: S1,
        shared_state: SharedState<S2>,
        block_source: Blocks,
        importer: BoxStream<
            anyhow::Result<(BlockHeight, <Blocks as BlockSource>::Block)>,
        >,
    ) -> Self {
        let mut task = Self {
            sync_from,
            api,
            storage,
            shared_state,
            block_source,
            importer,
            last_seen_importer_height: None,
            old_and_new_block_stream: futures::stream::empty().into_boxed(),
        };

        let _ = task.restart_blocks_stream();

        task
    }

    pub fn restart_blocks_stream(&mut self) -> TaskNextAction {
        let importer_height = self.last_seen_importer_height;

        let next_height = try_or_stop!(self.shared_state.get_current_height())
            .and_then(|height| height.succ())
            .unwrap_or(self.sync_from);

        let old_blocks_stream =
            futures::stream::iter(self.block_source.blocks_starting_from(next_height))
                .take_while(move |result| {
                    let take = match result {
                        Ok((block_height, _)) => {
                            if let Some(importer_height) = importer_height {
                                *block_height < importer_height
                            } else {
                                true
                            }
                        }
                        Err(_) => true,
                    };

                    async move { take }
                });

        let receiver = self.shared_state.blocks_broadcast.subscribe();
        let life_stream =
            tokio_stream::wrappers::BroadcastStream::new(receiver).map(|result| {
                result.map_err(|err| {
                    Error::BlockSource(anyhow::anyhow!("Stream broadcast error: {err:?}"))
                })
            });

        let stream = old_blocks_stream.chain(life_stream).into_boxed();
        self.old_and_new_block_stream = stream;

        TaskNextAction::Continue
    }

    pub async fn handle_block(
        &mut self,
        block_height: BlockHeight,
        block: <Blocks as BlockSource>::Block,
    ) -> TaskNextAction {
        let next_height = try_or_stop!(self.shared_state.get_current_height())
            .and_then(|height| height.succ())
            .unwrap_or(self.sync_from);

        if next_height != block_height {
            tracing::warn!(
                "Received block at height {block_height} but expected height {next_height}. Restarting the blocks stream."
            );
            return self.restart_blocks_stream();
        }

        let res = self.storage.store_block(block_height, &block).await;
        match res {
            Ok(_) => TaskNextAction::Continue,
            // If we have an error, it means height is not updated in DB, and it will trigger
            // restart of the stream in next iteration.
            Err(err) => TaskNextAction::ErrorContinue(anyhow::anyhow!(err)),
        }
    }
}

impl<S1, S2, Blocks> RunnableTask for Task<S1, S2, Blocks>
where
    S1: BlocksStorage<Block = Blocks::Block>,
    S2: BlocksProvider<Block = Blocks::Block>,
    Blocks: BlockSource,
    <Blocks as BlockSource>::Block: Clone + Send + Sync + Debug,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => TaskNextAction::Stop,

            _ = self.api.await_stop() => TaskNextAction::Stop,

            block_res = self.importer.next() => {
                match block_res {
                    Some(res) => {
                        let (height, block) = try_or_stop!(res);

                        // The new block is added to the stream of old and new blocks and will be
                        // processed later during future iterations.
                        let _ = self
                            .shared_state
                            .blocks_broadcast
                            .send((height, block));
                        self.last_seen_importer_height = Some(height);
                        TaskNextAction::Continue
                    }
                    None => {
                        TaskNextAction::Stop
                    }
                }
            },
            event = self.old_and_new_block_stream.next() => {
                match event {
                    Some(Ok((block_height, block))) => {
                        self.handle_block(block_height, block).await
                    }
                    Some(Err(err)) => {
                        tracing::warn!("Error handling block: {err}, restarting the blocks stream");
                        self.restart_blocks_stream()
                    }
                    None => {
                        self.restart_blocks_stream()
                    }
                }
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        self.api.stop_and_await().await?;
        Ok(())
    }
}
