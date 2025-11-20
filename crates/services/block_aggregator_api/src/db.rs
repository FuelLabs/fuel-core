use crate::{blocks::BlockSourceEvent, result::Result};
use fuel_core_types::fuel_types::BlockHeight;

pub mod remote_cache;
pub mod storage_db;

pub mod storage_or_remote_db;
pub mod table;

/// The definition of the block aggregator database.
pub trait BlockAggregatorDB: Send + Sync {
    type Block;
    /// The type used to report a range of blocks
    type BlockRangeResponse;

    /// Stores a block with the given ID
    fn store_block(
        &mut self,
        block: BlockSourceEvent<Self::Block>,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Retrieves a range of blocks from the database
    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> impl Future<Output = Result<Self::BlockRangeResponse>> + Send;

    /// Retrieves the current height of the aggregated blocks If there is a break in the blocks,
    /// i.e. the blocks are being aggregated out of order, return the height of the last
    /// contiguous block
    fn get_current_height(
        &self,
    ) -> impl Future<Output = Result<Option<BlockHeight>>> + Send;
}
