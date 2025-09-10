use crate::blocks::{
    BlockSourceEvent,
    importer_and_db_source::BlockSerializer,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
    try_or_continue,
    try_or_stop,
};
use fuel_core_types::services::block_importer::SharedImportResult;
use futures::StreamExt;
use tokio::sync::mpsc::Sender;

pub struct InnerTask<Serializer> {
    pub(crate) importer: BoxStream<SharedImportResult>,
    // db: DB,
    pub(crate) serializer: Serializer,
    pub(crate) block_return: Sender<BlockSourceEvent>,
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
                    try_or_stop!(res, |_e| "failed to send imported block to receiver: {_e:?}");
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
