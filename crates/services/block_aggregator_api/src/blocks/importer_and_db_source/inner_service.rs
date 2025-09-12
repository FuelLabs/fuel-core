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
    self,
    StorageInspect,
    tables::{
        FuelBlocks,
        Transactions,
    },
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::TxId,
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
    _sync_task_handle: tokio::task::JoinHandle<bool>,
    sync_task_receiver: Receiver<FuelBlock>,
    _marker: PhantomData<DB>,
}

impl<Serializer, DB> InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send,
    DB: StorageInspect<FuelBlocks> + Send + 'static,
    DB: StorageInspect<Transactions> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
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
        let (_sync_task_handle, sync_task_receiver) =
            Self::sync_task_handle(db, db_starting_height, db_ending_height);
        Self {
            importer,
            serializer,
            block_return_sender: block_return,
            _sync_task_handle,
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
            tracing::debug!(
                "running sync task from height {} to {}",
                db_starting_height,
                db_ending_height
            );
            let start = u32::from(db_starting_height);
            let end = u32::from(db_ending_height);
            for height in start..=end {
                let height = BlockHeight::new(height);
                let res = StorageInspect::<FuelBlocks>::get(&db, &height);
                match res {
                    Ok(Some(compressed_block)) => {
                        tracing::debug!("found block at height {}, syncing", height);
                        let tx_ids = compressed_block.transactions();
                        let mut txs = Vec::new();
                        for tx_id in tx_ids {
                            let tx_res = StorageInspect::<Transactions>::get(&db, &tx_id);
                            match tx_res {
                                Ok(Some(tx)) => {
                                    tracing::debug!("found tx id: {:?}", tx_id);
                                    txs.push(tx.into_owned());
                                }
                                Ok(None) => {
                                    tracing::debug!("tx id not found in db: {:?}", tx_id);
                                    todo!()
                                }
                                Err(_) => {
                                    tracing::debug!(
                                        "error while finding tx: {:?}",
                                        tx_id
                                    );
                                    todo!()
                                }
                            }
                        }
                        let block = <fuel_core_types::blockchain::block::Block<TxId> as Clone>::clone(&compressed_block).uncompress(txs);
                        let _res = sync_task_sender.send(block).await.unwrap();
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
    Serializer: BlockSerializer + Send + Sync,
    DB: Send + Sync + 'static,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            fuel_block = self.importer.next() => self.process_shared_import_result(fuel_block).await,
            fuel_block = self.sync_task_receiver.recv() => self.process_db_block(fuel_block).await,
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            },
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<Serializer, DB> InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync,
    DB: Send + Sync + 'static,
{
    async fn process_shared_import_result(
        &self,
        maybe_import_result: Option<SharedImportResult>,
    ) -> TaskNextAction {
        tracing::debug!("imported block");
        match maybe_import_result {
            Some(import_result) => {
                let height = import_result.sealed_block.entity.header().height();
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

    async fn process_db_block(
        &self,
        maybe_fuel_block: Option<FuelBlock>,
    ) -> TaskNextAction {
        tracing::debug!("synced block from db");
        match maybe_fuel_block {
            Some(fuel_block) => {
                let height = fuel_block.header().height();
                let res = self.serializer.serialize_block(&fuel_block);
                let block = try_or_continue!(res);
                let event = BlockSourceEvent::NewBlock(*height, block);
                let res = self.block_return_sender.send(event).await;
                try_or_stop!(res, |_e| "failed to send db block to receiver: {_e:?}");
                TaskNextAction::Continue
            }
            None => {
                tracing::debug!("sync task returned None, stopping");
                TaskNextAction::Stop
            }
        }
    }
}

#[async_trait::async_trait]
impl<Serializer, DB> RunnableService for InnerTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    DB: Send + Sync + 'static,
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
