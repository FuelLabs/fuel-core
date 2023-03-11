use crate::graphql_api::ports::DatabasePort;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
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
use fuel_core_types::blockchain::{
    block::CompressedBlock,
    consensus::Consensus,
    primitives::{
        BlockHeight,
        BlockId,
    },
};

pub trait SimpleBlockData: Send + Sync {
    fn block(&self, id: &BlockId) -> StorageResult<CompressedBlock>;
}

impl<D: DatabasePort + ?Sized> SimpleBlockData for D {
    fn block(&self, id: &BlockId) -> StorageResult<CompressedBlock> {
        let block = self
            .storage::<FuelBlocks>()
            .get(id)?
            .ok_or_else(|| not_found!(FuelBlocks))?
            .into_owned();

        Ok(block)
    }
}

pub trait BlockQueryData: Send + Sync + SimpleBlockData {
    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId>;

    fn latest_block_id(&self) -> StorageResult<BlockId>;

    fn latest_block_height(&self) -> StorageResult<BlockHeight>;

    fn latest_block(&self) -> StorageResult<CompressedBlock>;

    fn compressed_blocks(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>>;

    fn consensus(&self, id: &BlockId) -> StorageResult<Consensus>;
}

impl<D: DatabasePort + ?Sized> BlockQueryData for D {
    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId> {
        self.block_id(height)
    }

    fn latest_block_id(&self) -> StorageResult<BlockId> {
        self.ids_of_latest_block().map(|(_, id)| id)
    }

    fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.ids_of_latest_block().map(|(height, _)| height)
    }

    fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.latest_block_id().and_then(|id| self.block(&id))
    }

    fn compressed_blocks(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>> {
        self.blocks_ids(start.map(Into::into), direction)
            .into_iter()
            .map(|result| {
                result.and_then(|(_, id)| {
                    let block = self.block(&id)?;

                    Ok(block)
                })
            })
            .into_boxed()
    }

    fn consensus(&self, id: &BlockId) -> StorageResult<Consensus> {
        self.storage::<SealedBlockConsensus>()
            .get(id)
            .map(|c| c.map(|c| c.into_owned()))?
            .ok_or(not_found!(SealedBlockConsensus))
    }
}
