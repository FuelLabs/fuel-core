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
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use futures::StreamExt;
use tokio::sync::mpsc::Sender;

pub struct ImporterTask<Serializer> {
    importer: BoxStream<SharedImportResult>,
    serializer: Serializer,
    block_return_sender: Sender<BlockSourceEvent>,
    new_end_sender: Option<tokio::sync::oneshot::Sender<BlockHeight>>,
}

impl<Serializer> ImporterTask<Serializer>
where
    Serializer: BlockSerializer + Send,
{
    pub fn new(
        importer: BoxStream<SharedImportResult>,
        serializer: Serializer,
        block_return: Sender<BlockSourceEvent>,
        new_end_sender: Option<tokio::sync::oneshot::Sender<BlockHeight>>,
    ) -> Self {
        Self {
            importer,
            serializer,
            block_return_sender: block_return,
            new_end_sender,
        }
    }
}
impl<Serializer> RunnableTask for ImporterTask<Serializer>
where
    Serializer: BlockSerializer + Send + Sync,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            fuel_block = self.importer.next() => self.process_shared_import_result(fuel_block).await,
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<Serializer> ImporterTask<Serializer>
where
    Serializer: BlockSerializer + Send + Sync,
{
    async fn process_shared_import_result(
        &mut self,
        maybe_import_result: Option<SharedImportResult>,
    ) -> TaskNextAction {
        tracing::debug!("imported block");
        match maybe_import_result {
            Some(import_result) => {
                let height = import_result.sealed_block.entity.header().height();
                if let Some(sender) = self.new_end_sender.take() {
                    match sender.send(*height) {
                        Ok(_) => {
                            tracing::debug!(
                                "sent new end height to sync task: {:?}",
                                height
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "failed to send new end height to sync task: {:?}",
                                e
                            );
                        }
                    }
                }
                let res = self
                    .serializer
                    .serialize_block(&import_result.sealed_block.entity);
                let block = try_or_continue!(res);
                let event = BlockSourceEvent::NewBlock(*height, block);
                let res = self.block_return_sender.send(event).await;
                try_or_stop!(
                    res,
                    |_e| "failed to send imported block to receiver: {_e:?}"
                );
                TaskNextAction::Continue
            }
            None => {
                tracing::debug!("importer returned None, stopping");
                TaskNextAction::Stop
            }
        }
    }
}

#[async_trait::async_trait]
impl<Serializer> RunnableService for ImporterTask<Serializer>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
{
    const NAME: &'static str = "BlockSourceImporterTask";
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
