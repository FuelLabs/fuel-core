use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_services::yield_stream::StreamYieldExt;
use fuel_core_storage::{
    iter::IterDirection,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::BlockHeightQuery,
    },
    fuel_types::BlockHeight,
};
use futures::Stream;

impl ReadView {
    pub fn latest_block_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    pub fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.block(&self.latest_block_height()?)
    }

    pub fn compressed_blocks(
        &self,
        height: BlockHeightQuery,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<CompressedBlock>> + '_ {
        futures::stream::iter(self.blocks(height, direction)).yield_each(self.batch_size)
    }
}
