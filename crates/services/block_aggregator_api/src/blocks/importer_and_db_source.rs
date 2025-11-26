use crate::{
    blocks::{
        BlockSource,
        BlockSourceEvent,
        importer_and_db_source::importer_service::ImporterTask,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_services::{
    Service,
    ServiceRunner,
    stream::BoxStream,
};
use fuel_core_storage::{
    Error as StorageError,
    StorageInspect,
    tables::FuelBlocks,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};

use crate::blocks::importer_and_db_source::sync_service::{
    SyncTask,
    TxReceipts,
};
use fuel_core_storage::tables::Transactions;
use fuel_core_types::fuel_tx::Receipt as FuelReceipt;

pub mod importer_service;
pub mod sync_service;
#[cfg(test)]
mod tests;

pub mod serializer_adapter;

pub trait BlockSerializer {
    type Block;
    fn serialize_block(
        &self,
        block: &FuelBlock,
        receipts: &[FuelReceipt],
    ) -> Result<Self::Block>;
}

/// A block source that combines an importer and a database sync task.
/// Old blocks will be synced from a target database and new blocks will be received from
/// the importer
pub struct ImporterAndDbSource<Serializer, DB, Receipts>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    <Serializer as BlockSerializer>::Block: Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
{
    importer_task: ServiceRunner<ImporterTask<Serializer, Serializer::Block>>,
    sync_task: ServiceRunner<SyncTask<Serializer, DB, Receipts, Serializer::Block>>,
    /// Receive blocks from the importer and sync tasks
    receiver: tokio::sync::mpsc::Receiver<BlockSourceEvent<Serializer::Block>>,
}

impl<Serializer, DB, Receipts> ImporterAndDbSource<Serializer, DB, Receipts>
where
    Serializer: BlockSerializer + Clone + Send + Sync + 'static,
    <Serializer as BlockSerializer>::Block: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError> + Send + Sync,
    DB: StorageInspect<Transactions, Error = StorageError> + Send + 'static,
    Receipts: TxReceipts,
{
    pub fn new(
        importer: BoxStream<SharedImportResult>,
        serializer: Serializer,
        db: DB,
        receipts: Receipts,
        db_starting_height: BlockHeight,
        db_ending_height: BlockHeight,
    ) -> Self {
        const ARB_CHANNEL_SIZE: usize = 100;
        let (block_return, receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let importer_task =
            ImporterTask::new(importer, serializer.clone(), block_return.clone());
        let importer_runner = ServiceRunner::new(importer_task);
        importer_runner.start().unwrap();
        let sync_task = SyncTask::new(
            serializer,
            block_return,
            db,
            receipts,
            db_starting_height,
            db_ending_height,
        );
        let sync_runner = ServiceRunner::new(sync_task);
        sync_runner.start().unwrap();
        Self {
            importer_task: importer_runner,
            sync_task: sync_runner,
            receiver,
        }
    }
}

impl<Serializer, DB, Receipts> BlockSource
    for ImporterAndDbSource<Serializer, DB, Receipts>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    <Serializer as BlockSerializer>::Block: Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks, Error = StorageError>,
    DB: StorageInspect<Transactions, Error = StorageError>,
    Receipts: TxReceipts,
{
    type Block = Serializer::Block;

    async fn next_block(&mut self) -> Result<BlockSourceEvent<Self::Block>> {
        tracing::debug!("awaiting next block");
        tokio::select! {
            block_res = self.receiver.recv() => {
                block_res.ok_or(Error::BlockSource(anyhow!("Block source channel closed")))
            }
            _ = self.importer_task.await_stop() => {
                Err(Error::BlockSource(anyhow!("Importer task stopped unexpectedly")))
            }
            _ = self.sync_task.await_stop() => {
                Err(Error::BlockSource(anyhow!("Sync task stopped unexpectedly")))
            }
            importer_error = self.importer_task.await_stop() => {
                Err(Error::BlockSource(anyhow!("Importer task stopped unexpectedly: {:?}", importer_error)))
            }
            sync_error = self.sync_task.await_stop() => {
                Err(Error::BlockSource(anyhow!("Sync task stopped unexpectedly: {:?}", sync_error)))
            }
        }
    }

    async fn drain(&mut self) -> Result<()> {
        self.importer_task.stop();
        self.sync_task.stop();
        self.receiver.close();
        Ok(())
    }
}
