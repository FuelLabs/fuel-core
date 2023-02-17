use crate::fuel_core_graphql_api::service::Database;
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

pub struct BlockQueryContext(pub Database);

pub trait BlockQueryData: Send + Sync {
    fn block(&self, id: &BlockId) -> StorageResult<CompressedBlock>;

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

impl BlockQueryData for BlockQueryContext {
    fn block(&self, id: &BlockId) -> StorageResult<CompressedBlock> {
        let db = self.0;

        let block = db
            .as_ref()
            .storage::<FuelBlocks>()
            .get(id)?
            .ok_or_else(|| not_found!(FuelBlocks))?
            .into_owned();

        Ok(block)
    }

    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId> {
        self.0.block_id(height)
    }

    fn latest_block_id(&self) -> StorageResult<BlockId> {
        self.0.ids_of_latest_block().map(|(_, id)| id)
    }

    fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.0.ids_of_latest_block().map(|(height, _)| height)
    }

    fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.latest_block_id().and_then(|id| self.block(&id))
    }

    fn compressed_blocks(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>> {
        let db = self.0;

        db.blocks_ids(start.map(Into::into), direction)
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
        self.0
            .as_ref()
            .storage::<SealedBlockConsensus>()
            .get(id)
            .map(|c| c.map(|c| c.into_owned()))?
            .ok_or(not_found!(SealedBlockConsensus))
    }
}
