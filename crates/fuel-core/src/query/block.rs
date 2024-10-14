use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};

impl ReadView {
    pub fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    pub fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.block(&self.latest_block_height()?)
    }

    pub fn compressed_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<CompressedBlock>> {
        self.blocks(height, direction)
    }
}
