use crate::blocks::{
    BlockSourceEvent,
    importer_and_db_source::BlockSerializer,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    try_or_continue,
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
    fuel_tx::TxId,
    fuel_types::BlockHeight,
};
use tokio::sync::mpsc::Sender;

pub struct SyncTask<Serializer, DB> {
    serializer: Serializer,
    block_return_sender: Sender<BlockSourceEvent>,
    db: DB,
    db_starting_height: BlockHeight,
    db_ending_height: BlockHeight,
}

impl<Serializer, DB> SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send,
    DB: StorageInspect<FuelBlocks> + Send + 'static,
    DB: StorageInspect<Transactions> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
{
    pub fn new(
        serializer: Serializer,
        block_return: Sender<BlockSourceEvent>,
        db: DB,
        db_starting_height: BlockHeight,
        db_ending_height: BlockHeight,
    ) -> Self {
        Self {
            serializer,
            block_return_sender: block_return,
            db,
            db_starting_height,
            db_ending_height,
        }
    }
}

impl<Serializer, DB> RunnableTask for SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks> + Send + 'static,
    DB: StorageInspect<Transactions> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
{
    // TODO: This is syncronous and then just ends. What do we want to do when this is done?
    async fn run(&mut self, _watcher: &mut StateWatcher) -> TaskNextAction {
        let start = u32::from(self.db_starting_height);
        // TODO: make this more dynamic so we can make sure we get all blocks up to what the importer receives
        let end = u32::from(self.db_ending_height);
        for height in start..=end {
            let height = BlockHeight::new(height);
            let res = StorageInspect::<FuelBlocks>::get(&self.db, &height);
            match res {
                Ok(Some(compressed_block)) => {
                    tracing::debug!("found block at height {}, syncing", height);
                    let tx_ids = compressed_block.transactions();
                    let mut txs = Vec::new();
                    for tx_id in tx_ids {
                        let tx_res =
                            StorageInspect::<Transactions>::get(&self.db, &tx_id);
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
                                tracing::debug!("error while finding tx: {:?}", tx_id);
                                todo!()
                            }
                        }
                    }
                    let block = <fuel_core_types::blockchain::block::Block<TxId> as Clone>::clone(&compressed_block).uncompress(txs);
                    let res = self.serializer.serialize_block(&block);
                    let block = try_or_continue!(res);
                    let event =
                        BlockSourceEvent::OldBlock(BlockHeight::from(*height), block);
                    self.block_return_sender.send(event).await.unwrap();
                }
                Ok(None) => {
                    tracing::warn!("no block found at height {}, skipping", height);
                }
                Err(e) => {
                    tracing::error!("error fetching block at height {}: {:?}", height, e);
                    return TaskNextAction::Stop;
                }
            }
        }
        TaskNextAction::Stop
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Serializer, DB> RunnableService for SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks> + Send + 'static,
    DB: StorageInspect<Transactions> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
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
