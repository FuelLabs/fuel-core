use crate::{
    blocks::{
        Block,
        BlockSource,
        BlockSourceEvent,
        importer_and_db_source::inner_service::InnerTask,
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
use fuel_core_types::{
    blockchain::SealedBlock as FuelBlock,
    services::block_importer::SharedImportResult,
};

pub mod inner_service;
#[cfg(test)]
mod tests;

pub trait BlockSerializer {
    fn serialize_block(&self, block: &FuelBlock) -> Result<Block>;
}

pub struct ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    _inner: ServiceRunner<InnerTask<Serializer>>,
    receiver: tokio::sync::mpsc::Receiver<BlockSourceEvent>,
}

impl<Serializer> ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
{
    pub fn new(importer: BoxStream<SharedImportResult>, serializer: Serializer) -> Self {
        const ARB_CHANNEL_SIZE: usize = 100;
        let (block_return, receiver) = tokio::sync::mpsc::channel(ARB_CHANNEL_SIZE);
        let inner = InnerTask {
            importer,
            serializer,
            block_return,
        };
        let runner = ServiceRunner::new(inner);
        runner.start().unwrap();
        Self {
            _inner: runner,
            receiver,
        }
    }
}

impl<Serializer> BlockSource for ImporterAndDbSource<Serializer>
where
    Serializer: BlockSerializer + Send + 'static,
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
