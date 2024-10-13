use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    iter::IterDirection,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};
use futures::{
    Stream,
    StreamExt,
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
    ) -> impl Stream<Item = StorageResult<CompressedBlock>> + '_ {
        futures::stream::iter(self.blocks(height, direction))
            .chunks(self.batch_size)
            .filter_map(|chunk| async move {
                // Give a chance to other tasks to run.
                tokio::task::yield_now().await;
                Some(futures::stream::iter(chunk))
            })
            .flatten()
    }
}
