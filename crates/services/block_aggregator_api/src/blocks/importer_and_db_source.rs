use crate::{
    blocks::{
        Block,
        BlockSource,
        BlockSourceEvent,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    Service,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
    try_or_continue,
    try_or_stop,
};
use fuel_core_types::{
    blockchain::SealedBlock as FuelBlock,
    services::block_importer::SharedImportResult,
};
use futures::StreamExt;
use tokio::sync::mpsc::Sender;

#[cfg(test)]
mod tests;

pub trait BlockSerializer {
    fn serialize_block(&self, block: &FuelBlock) -> Result<Block>;
}

pub struct ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    inner: ServiceRunner<InnerTask<Serializer>>,
    receiver: tokio::sync::mpsc::Receiver<BlockSourceEvent>,
}

impl<Serializer> ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    pub fn new(importer: BoxStream<SharedImportResult>, serializer: Serializer) -> Self {
        const ARB_CHANNEL_SIZE: usize = 100;
        let (block_return, receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let inner = InnerTask {
            importer,
            serializer,
            block_return,
        };
        let mut runner = ServiceRunner::new(inner);
        runner.start().unwrap();
        Self {
            inner: runner,
            receiver,
        }
    }
}

impl<Serializer> BlockSource for ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    async fn next_block(&mut self) -> Result<BlockSourceEvent> {
        tracing::debug!("awaiting next block");
        self.receiver
            .recv()
            .await
            .ok_or(Error::BlockSource(anyhow!("Block source channel closed")))
    }

    async fn drain(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct InnerTask<Serializer> {
    importer: BoxStream<SharedImportResult>,
    // db: DB,
    serializer: Serializer,
    block_return: Sender<BlockSourceEvent>,
}

impl<Serializer> RunnableTask for InnerTask<Serializer>
where
    Serializer: BlockSerializer + Send,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            fuel_block = self.importer.next() => {
                tracing::debug!("imported block");
                if let Some(inner) = fuel_block {
                    let height = inner.sealed_block.entity.header().height();
                    let res = self.serializer.serialize_block(&inner.sealed_block);
                    let block = try_or_continue!(res);
                    let event = BlockSourceEvent::NewBlock(*height, block);
                    let res = self.block_return.send(event).await;
                    try_or_stop!(res, |e| "failed to send imported block to receiver: {e:?}");
                    TaskNextAction::Continue
                } else {
                    tracing::debug!("importer stream ended");
                    TaskNextAction::Stop
                }
            }
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },
            // fuel_block = self.db.next_block() => {
            //     todo!()
            // }
            // serialized_block = self.serializer.next_serialized_block() => {
            //     let res = self.block_return.send(serialized_block);
            //     try_or_stop!(res)
            // }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        todo!()
    }
}

#[async_trait::async_trait]
impl<Serializer> RunnableService for InnerTask<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    const NAME: &'static str = "BlockSourceInnerService";
    type SharedData = ();
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        ()
    }

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}
