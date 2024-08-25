use crate::fuel_core_graphql_api::ports::DatabaseBlocks;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
    },
    fuel_types::BlockHeight,
};

pub trait SimpleBlockData: Send + Sync {
    fn block(&self, id: &BlockHeight) -> StorageResult<CompressedBlock>;
}

impl<D> SimpleBlockData for D
where
    D: DatabaseBlocks + ?Sized + Send + Sync,
{
    fn block(&self, id: &BlockHeight) -> StorageResult<CompressedBlock> {
        self.block(id)
    }
}

pub trait BlockQueryData: Send + Sync + SimpleBlockData {
    fn latest_block_height(&self) -> StorageResult<BlockHeight>;

    fn latest_block(&self) -> StorageResult<CompressedBlock>;

    fn compressed_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>>;

    fn consensus(&self, id: &BlockHeight) -> StorageResult<Consensus>;
}

impl<D> BlockQueryData for D
where
    D: DatabaseBlocks + ?Sized + Send + Sync,
{
    fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.block(&self.latest_block_height()?)
    }

    fn compressed_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>> {
        self.blocks(height, direction)
    }

    fn consensus(&self, id: &BlockHeight) -> StorageResult<Consensus> {
        self.consensus(id)
    }
}
