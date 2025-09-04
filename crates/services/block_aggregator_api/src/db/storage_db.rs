use crate::{
    block_range_response::BlockRangeResponse,
    blocks::Block,
    db::BlockAggregatorDB,
    result::Result,
};

pub struct StorageDB;

impl BlockAggregatorDB for StorageDB {
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
