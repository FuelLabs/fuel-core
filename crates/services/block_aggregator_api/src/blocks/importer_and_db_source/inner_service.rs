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
use fuel_core_storage::{
    StorageInspect,
    column::Column as OnChainColumn,
    kv_store::KeyValueInspect,
    tables::FuelBlocks,
    transactional::AtomicView,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};
use futures::StreamExt;
use std::marker::PhantomData;
use tokio::sync::mpsc::{
    Receiver,
    Sender,
};

pub struct InnerTask<Serializer, DB> {
    importer: BoxStream<SharedImportResult>,
    serializer: Serializer,
    block_return_sender: Sender<BlockSourceEvent>,
    sync_task_handle: tokio::task::JoinHandle<bool>,
    sync_task_receiver: Receiver<FuelBlock>,
    _marker: PhantomData<DB>,
}

impl<Serializer, DB> InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send,
    DB: StorageInspect<FuelBlocks> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug,
{
    pub fn new(
        importer: BoxStream<SharedImportResult>,
        serializer: Serializer,
        block_return: Sender<BlockSourceEvent>,
        db: DB,
        db_starting_height: BlockHeight,
        db_ending_height: BlockHeight,
    ) -> Self {
        // TODO: Should this be its own service?
        let (sync_task_handle, sync_task_receiver) =
            Self::sync_task_handle(db, db_starting_height, db_ending_height);
        Self {
            importer,
            serializer,
            block_return_sender: block_return,
            sync_task_handle,
            sync_task_receiver,
            _marker: PhantomData,
        }
    }

    fn sync_task_handle(
        db: DB,
        db_starting_height: BlockHeight,
        db_ending_height: BlockHeight,
    ) -> (tokio::task::JoinHandle<bool>, Receiver<FuelBlock>) {
        const ARB_CHANNEL_SIZE: usize = 100;
        let (sync_task_sender, sync_task_receiver) =
            tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let sync_task_handle = tokio::spawn(async move {
            let start = u32::from(db_starting_height);
            let end = u32::from(db_ending_height);
            for height in start..=end {
                let height = BlockHeight::new(height);
                let res = StorageInspect::<FuelBlocks>::get(&db, &height);
                match res {
                    Ok(Some(compressed_block)) => {
                        let block = todo!();
                        // let send_res = sync_task_sender.send(block).await;
                        // if send_res.is_err() {
                        //     tracing::warn!(
                        //         "sync task receiver dropped, stopping sync task"
                        //     );
                        //     return false
                        // }
                    }
                    Ok(None) => {
                        tracing::warn!("no block found at height {}, skipping", height);
                    }
                    Err(e) => {
                        tracing::error!(
                            "error fetching block at height {}: {:?}",
                            height,
                            e
                        );
                        return false
                    }
                }
            }
            true
        });
        (sync_task_handle, sync_task_receiver)
    }
}

impl<Serializer, DB> RunnableTask for InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send,
    DB: Send + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            fuel_block = self.importer.next() => {
                tracing::debug!("imported block");
                if let Some(inner) = fuel_block {
                    let height = inner.sealed_block.entity.header().height();
                    let res = self.serializer.serialize_block(&inner.sealed_block.entity);
                    let block = try_or_continue!(res);
                    let event = BlockSourceEvent::NewBlock(*height, block);
                    let res = self.block_return_sender.send(event).await;
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
impl<Serializer, DB> RunnableService for InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + 'static,
    DB: Send + 'static,
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
