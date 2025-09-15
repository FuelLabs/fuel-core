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
    db_ending_height: Option<BlockHeight>,
    new_ending_height: tokio::sync::oneshot::Receiver<BlockHeight>,
}

impl<Serializer, DB, E> SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send,
    DB: StorageInspect<FuelBlocks, Error = E> + Send + 'static,
    DB: StorageInspect<Transactions, Error = E> + Send + 'static,
    E: std::fmt::Debug + Send,
{
    pub fn new(
        serializer: Serializer,
        block_return: Sender<BlockSourceEvent>,
        db: DB,
        db_starting_height: BlockHeight,
        db_ending_height: Option<BlockHeight>,
        new_ending_height: tokio::sync::oneshot::Receiver<BlockHeight>,
    ) -> Self {
        Self {
            serializer,
            block_return_sender: block_return,
            db,
            db_starting_height,
            db_ending_height,
            new_ending_height,
        }
    }

    async fn check_for_new_end(&mut self) -> Option<BlockHeight> {
        self.new_ending_height.try_recv().ok()
    }

    async fn get_block(
        &mut self,
        height: &BlockHeight,
    ) -> Result<Option<fuel_core_types::blockchain::block::Block>, E> {
        let maybe_block = StorageInspect::<FuelBlocks>::get(&self.db, height)?;
        if let Some(block) = maybe_block {
            let tx_ids = block.transactions();
            let mut txs = Vec::new();
            for tx_id in tx_ids {
                let tx_res = StorageInspect::<Transactions>::get(&self.db, &tx_id);
                match tx_res {
                    Ok(Some(tx)) => {
                        tracing::debug!("found tx id: {:?}", tx_id);
                        txs.push(tx.into_owned());
                    }
                    Ok(None) => {
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                }
            }
            let block =
                <fuel_core_types::blockchain::block::Block<TxId> as Clone>::clone(&block)
                    .uncompress(txs);
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }
}

impl<Serializer, DB, E> RunnableTask for SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = E> + Send + 'static,
    DB: StorageInspect<Transactions, Error = E> + Send + 'static,
    E: std::fmt::Debug + Send,
{
    // TODO: This is synchronous and then just ends. What do we want to do when this is done?
    async fn run(&mut self, _watcher: &mut StateWatcher) -> TaskNextAction {
        if let Some(new_end) = self.check_for_new_end().await {
            self.db_ending_height = Some(new_end);
        }
        if let Some(current_end) = self.db_ending_height {
            if self.db_starting_height >= current_end {
                tracing::info!("reached end height {}, stopping sync task", current_end);
                return TaskNextAction::Stop;
            }
        }
        let next_height = self.db_starting_height;
        match self.get_block(&next_height).await {
            Ok(Some(block)) => {
                let res = self.serializer.serialize_block(&block);
                let block = try_or_continue!(res);
                let event =
                    BlockSourceEvent::OldBlock(BlockHeight::from(*next_height), block);
                let res = self.block_return_sender.send(event).await;
                try_or_continue!(res);
                self.db_starting_height =
                    BlockHeight::from((*next_height).saturating_add(1));
                TaskNextAction::Continue
            }
            Ok(None) => {
                tracing::warn!("no block found at height {:?}, retrying", next_height);
                TaskNextAction::Continue
            }
            Err(e) => {
                tracing::error!(
                    "error fetching block at height {}: {:?}",
                    next_height,
                    e
                );
                TaskNextAction::Stop
            }
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Serializer, DB, E> RunnableService for SyncTask<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = E> + Send + 'static,
    DB: StorageInspect<Transactions, Error = E> + Send + 'static,
    E: std::fmt::Debug + Send,
{
    const NAME: &'static str = "BlockSourceSyncTask";
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
