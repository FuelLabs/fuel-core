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
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
};
use tokio::sync::mpsc::Sender;

pub struct SyncTask<Serializer, DB> {
    serializer: Serializer,
    block_return_sender: Sender<BlockSourceEvent>,
    db: DB,
    next_height: BlockHeight,
    maybe_stop_height: Option<BlockHeight>,
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
            next_height: db_starting_height,
            maybe_stop_height: db_ending_height,
            new_ending_height,
        }
    }

    async fn maybe_update_stop_height(&mut self) {
        if let Ok(last_height) = self.new_ending_height.try_recv() {
            tracing::info!("updating last height to {}", last_height);
            self.maybe_stop_height = Some(last_height);
        }
    }

    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> Result<Option<fuel_core_types::blockchain::block::Block>, E> {
        let maybe_block = StorageInspect::<FuelBlocks>::get(&self.db, height)?;
        if let Some(block) = maybe_block {
            let tx_ids = block.transactions();
            let txs = self.get_txs(tx_ids)?;
            let block =
                <fuel_core_types::blockchain::block::Block<TxId> as Clone>::clone(&block)
                    .uncompress(txs);
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    fn get_txs(&self, tx_ids: &[TxId]) -> Result<Vec<Transaction>, E> {
        let mut txs = Vec::new();
        for tx_id in tx_ids {
            match StorageInspect::<Transactions>::get(&self.db, tx_id)? {
                Some(tx) => {
                    tracing::debug!("found tx id: {:?}", tx_id);
                    txs.push(tx.into_owned());
                }
                None => {
                    return Ok(vec![]);
                }
            }
        }
        Ok(txs)
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
    async fn run(&mut self, _watcher: &mut StateWatcher) -> TaskNextAction {
        self.maybe_update_stop_height().await;
        if let Some(last_height) = self.maybe_stop_height {
            if self.next_height >= last_height {
                tracing::info!(
                    "reached end height {}, putting task into hibernation",
                    last_height
                );
                futures::future::pending().await
            }
        }
        let next_height = self.next_height;
        let res = self.get_block(&next_height);
        let maybe_block = try_or_stop!(res, |e| {
            tracing::error!("error fetching block at height {}: {:?}", next_height, e);
        });
        if let Some(block) = maybe_block {
            let res = self.serializer.serialize_block(&block);
            let block = try_or_continue!(res);
            let event =
                BlockSourceEvent::OldBlock(BlockHeight::from(*next_height), block);
            let res = self.block_return_sender.send(event).await;
            try_or_continue!(res);
            self.next_height = BlockHeight::from((*next_height).saturating_add(1));
        } else {
            tracing::warn!("no block found at height {:?}, retrying", next_height);
        }
        TaskNextAction::Continue
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

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(
        self,
        _state_watcher: &StateWatcher,
        _params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}
