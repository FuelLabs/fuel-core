use crate::{
    blocks::{
        BlockSourceEvent,
        importer_and_db_source::BlockSerializer,
    },
    result::{
        Error,
        Result,
    },
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
    Error as StorageError,
    StorageInspect,
    tables::{
        FuelBlocks,
        Transactions,
    },
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
};
use futures::{
    StreamExt,
    TryStreamExt,
    stream::FuturesOrdered,
};
use tokio::sync::mpsc::Sender;

pub struct SyncTask<Serializer, DB, Receipts, B> {
    serializer: Serializer,
    block_return_sender: Sender<BlockSourceEvent<B>>,
    db: DB,
    receipts: Receipts,
    next_height: BlockHeight,
    // exclusive, does not ask for this block
    stop_height: BlockHeight,
}

pub trait TxReceipts: 'static + Send + Sync {
    fn get_receipts(
        &self,
        tx_id: &TxId,
    ) -> impl Future<Output = Result<Vec<Receipt>>> + Send;
}

impl<Serializer, DB, Receipts> SyncTask<Serializer, DB, Receipts, Serializer::Block>
where
    Serializer: BlockSerializer + Send,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
{
    pub fn new(
        serializer: Serializer,
        block_return: Sender<BlockSourceEvent<Serializer::Block>>,
        db: DB,
        receipts: Receipts,
        db_starting_height: BlockHeight,
        // does not ask for this block (exclusive)
        db_ending_height: BlockHeight,
    ) -> Self {
        Self {
            serializer,
            block_return_sender: block_return,
            db,
            receipts,
            next_height: db_starting_height,
            stop_height: db_ending_height,
        }
    }

    async fn get_block_and_receipts(
        &self,
        height: &BlockHeight,
    ) -> Result<Option<(FuelBlock, Vec<Vec<Receipt>>)>> {
        let maybe_block = StorageInspect::<FuelBlocks>::get(&self.db, height)
            .map_err(Error::block_source_error)?;
        if let Some(block) = maybe_block {
            let tx_ids = block.transactions();
            let txs = self.get_txs(tx_ids)?;
            let receipts = self.get_receipts(tx_ids).await?;
            let block = block.into_owned().uncompress(txs);
            Ok(Some((block, receipts)))
        } else {
            Ok(None)
        }
    }

    fn get_txs(&self, tx_ids: &[TxId]) -> Result<Vec<Transaction>> {
        let mut txs = Vec::new();
        for tx_id in tx_ids {
            match StorageInspect::<Transactions>::get(&self.db, tx_id)
                .map_err(Error::block_source_error)?
            {
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

    async fn get_receipts(&self, tx_ids: &[TxId]) -> Result<Vec<Vec<Receipt>>> {
        let receipt_futs = tx_ids.iter().map(|tx_id| self.receipts.get_receipts(tx_id));
        FuturesOrdered::from_iter(receipt_futs)
            .then(|res| async move { res.map_err(Error::block_source_error) })
            .try_collect()
            .await
    }
}

impl<Serializer, DB, Receipts> RunnableTask
    for SyncTask<Serializer, DB, Receipts, Serializer::Block>
where
    Serializer: BlockSerializer + Send + Sync,
    Serializer::Block: Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
{
    async fn run(&mut self, _watcher: &mut StateWatcher) -> TaskNextAction {
        if self.next_height >= self.stop_height {
            tracing::info!(
                "reached stop height {}, putting task into hibernation",
                self.stop_height
            );
            let _ = _watcher.while_started().await;
            return TaskNextAction::Stop
        }
        let next_height = self.next_height;
        let res = self.get_block_and_receipts(&next_height).await;
        let maybe_block_and_receipts = try_or_stop!(res, |e| {
            tracing::error!("error fetching block at height {}: {:?}", next_height, e);
        });
        if let Some((block, receipts)) = maybe_block_and_receipts {
            tracing::debug!(
                "found block at height {:?}, sending to return channel",
                next_height
            );
            let res = self.serializer.serialize_block(&block, &receipts);
            let block = try_or_continue!(res);
            let event =
                BlockSourceEvent::OldBlock(BlockHeight::from(*next_height), block);
            let res = self.block_return_sender.send(event).await;
            try_or_continue!(res);
            self.next_height = BlockHeight::from((*next_height).saturating_add(1));
            TaskNextAction::Continue
        } else {
            tracing::error!("no block found at height {:?}, retrying", next_height);
            TaskNextAction::Stop
        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Serializer, DB, Receipts> RunnableService
    for SyncTask<Serializer, DB, Receipts, Serializer::Block>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    <Serializer as BlockSerializer>::Block: Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
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
