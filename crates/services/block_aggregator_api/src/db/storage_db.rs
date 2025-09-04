use crate::{
    block_range_response::BlockRangeResponse,
    blocks::Block,
    db::BlockAggregatorDB,
    result::Result,
};
use fuel_core_storage::{
    Error as StorageError,
    StorageMutate,
    transactional::{
        Modifiable,
        StorageTransaction,
    },
};
use fuel_core_types::fuel_types::BlockHeight;
use table::Blocks;

pub mod table;
#[cfg(test)]
mod tests;

pub struct StorageDB<S> {
    _inner: S,
}

impl<S> StorageDB<S> {
    pub fn new(storage: S) -> Self {
        Self { _inner: storage }
    }
}

impl<S> BlockAggregatorDB for StorageDB<S>
where
    S: Send + Sync + Modifiable,
    for<'a> StorageTransaction<&'a mut S>: StorageMutate<Blocks, Error = StorageError>,
{
    type BlockRange = BlockRangeResponse;

    async fn store_block(&mut self, _height: BlockHeight, _block: Block) -> Result<()> {
        todo!()
    }

    async fn get_block_range(
        &self,
        _first: BlockHeight,
        _last: BlockHeight,
    ) -> Result<Self::BlockRange> {
        todo!()
    }

    async fn get_current_height(&self) -> Result<BlockHeight> {
        todo!()
    }
}
