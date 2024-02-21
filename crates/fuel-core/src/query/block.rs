use crate::fuel_core_graphql_api::ports::OnChainDatabase;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    not_found,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    Result as StorageResult,
    StorageAsRef,
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

impl<D: OnChainDatabase + ?Sized> SimpleBlockData for D {
    fn block(&self, id: &BlockHeight) -> StorageResult<CompressedBlock> {
        let block = self
            .storage::<FuelBlocks>()
            .get(id)?
            .ok_or_else(|| not_found!(FuelBlocks))?
            .into_owned();

        Ok(block)
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

impl<D: OnChainDatabase + ?Sized> BlockQueryData for D {
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
        self.storage::<SealedBlockConsensus>()
            .get(id)
            .map(|c| c.map(|c| c.into_owned()))?
            .ok_or(not_found!(SealedBlockConsensus))
    }
}
