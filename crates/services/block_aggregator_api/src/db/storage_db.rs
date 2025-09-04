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
use table::Blocks;

pub mod table;
#[cfg(test)]
mod tests;

pub struct StorageDB<S> {
    _inner: S,
}

impl<S> BlockAggregatorDB for StorageDB<S>
where
    S: Send + Sync + Modifiable,
    for<'a> StorageTransaction<&'a mut S>: StorageMutate<Blocks, Error = StorageError>,
{
    type BlockRange = BlockRangeResponse;

    async fn store_block(&mut self, _id: u64, _block: Block) -> Result<()> {
        todo!()
    }

    async fn get_block_range(&self, _first: u64, _last: u64) -> Result<Self::BlockRange> {
        todo!()
    }

    async fn get_current_height(&self) -> Result<u64> {
        todo!()
    }
}
