use crate::{
    fuel_core_graphql_api::service::Database,
    state::IterDirection,
};
use fuel_core_storage::{
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

pub struct BlockQueryContext<'a>(pub &'a Database);

impl BlockQueryContext<'_> {
    pub fn block(&self, id: &BlockId) -> StorageResult<CompressedBlock> {
        let db = self.0;

        let block = db
            .as_ref()
            .storage::<FuelBlocks>()
            .get(id)?
            .ok_or_else(|| not_found!(FuelBlocks))?
            .into_owned();

        Ok(block)
    }

    pub fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId> {
        self.0.block_id(height)
    }

    pub fn latest_block_id(&self) -> StorageResult<BlockId> {
        self.0.ids_of_latest_block().map(|(_, id)| id)
    }

    pub fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.0.ids_of_latest_block().map(|(height, _)| height)
    }

    pub fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.latest_block_id().and_then(|id| self.block(&id))
    }

    pub fn compressed_blocks(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<CompressedBlock>> + '_ {
        let db = self.0;
        db.blocks_ids(start.map(Into::into), direction)
            .into_iter()
            .map(|result| {
                result.and_then(|(_, id)| {
                    let block = self.block(&id)?;

                    Ok(block)
                })
            })
    }

    pub fn consensus(&self, id: &BlockId) -> StorageResult<Consensus> {
        self.0
            .as_ref()
            .storage::<SealedBlockConsensus>()
            .get(id)
            .map(|c| c.map(|c| c.into_owned()))?
            .ok_or(not_found!(SealedBlockConsensus))
    }
}
