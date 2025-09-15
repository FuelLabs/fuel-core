use crate::{
    blocks::{
        Block,
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
    StorageInspect,
    tables::FuelBlocks,
};
use fuel_core_types::{
    blockchain::block::Block as FuelBlock,
    fuel_types::BlockHeight,
    services::block_importer::SharedImportResult,
};

use crate::blocks::importer_and_db_source::sync_service::SyncTask;
use fuel_core_storage::tables::Transactions;

pub mod importer_service;
pub mod sync_service;
#[cfg(test)]
mod tests;

pub trait BlockSerializer {
    fn serialize_block(&self, block: &FuelBlock) -> Result<Block>;
}

pub struct ImporterAndDbSource<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    DB: Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks>,
    DB: StorageInspect<Transactions>,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
{
    // TODO: How to handle errors from these tasks?
    _importer_task: ServiceRunner<ImporterTask<Serializer>>,
    _sync_task: ServiceRunner<SyncTask<Serializer, DB>>,
    /// Receive blocks from the importer and sync tasks
    receiver: tokio::sync::mpsc::Receiver<BlockSourceEvent>,
}

impl<Serializer, DB> ImporterAndDbSource<Serializer, DB>
where
    Serializer: BlockSerializer + Clone + Send + Sync + 'static,
    DB: StorageInspect<FuelBlocks> + Send + Sync,
    DB: StorageInspect<Transactions> + Send + 'static,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
{
    pub fn new(
        importer: BoxStream<SharedImportResult>,
        serializer: Serializer,
        database: DB,
        db_starting_height: BlockHeight,
        db_ending_height: Option<BlockHeight>,
    ) -> Self {
        const ARB_CHANNEL_SIZE: usize = 100;
        let (block_return, receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let (new_end_sender, new_end_receiver) = tokio::sync::oneshot::channel();
        let importer_task = ImporterTask::new(
            importer,
            serializer.clone(),
            block_return.clone(),
            Some(new_end_sender),
        );
        let importer_runner = ServiceRunner::new(importer_task);
        importer_runner.start().unwrap();
        let sync_task = SyncTask::new(
            serializer,
            block_return,
            database,
            db_starting_height,
            db_ending_height,
            new_end_receiver,
        );
        let sync_runner = ServiceRunner::new(sync_task);
        sync_runner.start().unwrap();
        Self {
            _importer_task: importer_runner,
            _sync_task: sync_runner,
            receiver,
        }
    }
}

impl<Serializer, DB> BlockSource for ImporterAndDbSource<Serializer, DB>
where
    Serializer: BlockSerializer + Send + Sync + 'static,
    DB: Send + Sync,
    DB: StorageInspect<FuelBlocks>,
    DB: StorageInspect<Transactions>,
    <DB as StorageInspect<FuelBlocks>>::Error: std::fmt::Debug + Send,
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
