use crate::{
    blocks::Block,
    result::Result,
};

pub mod storage_db;

/// The definition of the block aggregator database.
pub trait BlockAggregatorDB: Send + Sync {
    /// The type used to report a range of blocks
    type BlockRange;

    /// Stores a block with the given ID
    fn store_block(
        &mut self,
        id: u64,
        block: Block,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Retrieves a range of blocks from the database
    fn get_block_range(
        &self,
        first: u64,
        last: u64,
    ) -> impl Future<Output = Result<Self::BlockRange>> + Send;

    /// Retrieves the current height of the aggregated blocks If there is a break in the blocks,
    /// i.e. the blocks are being aggregated out of order, return the height of the last
    /// contiguous block
    fn get_current_height(&self) -> impl Future<Output = Result<u64>> + Send;
}
