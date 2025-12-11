use crate::result::Result;
use fuel_core_types::fuel_types::BlockHeight;

pub mod remote_cache;
pub mod storage_db;

pub mod storage_or_remote_db;
pub mod table;

pub trait BlocksProvider: Send + Sync + 'static {
    type Block: Send + Sync + 'static;
    /// The type used to report a range of blocks
    type BlockRangeResponse;

    /// Retrieves a range of blocks from the database
    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<Self::BlockRangeResponse>;

    /// Retrieves the current height of the aggregated blocks If there is a break in the blocks,
    /// i.e. the blocks are being aggregated out of order, return the height of the last
    /// contiguous block
    fn get_current_height(&self) -> Result<Option<BlockHeight>>;
}

/// The definition of the block aggregator database.
pub trait BlocksStorage: Send + Sync {
    type Block;
    /// The type used to report a range of blocks
    type BlockRangeResponse;

    /// Stores a block with the given ID
    fn store_block(
        &mut self,
        block_height: BlockHeight,
        block: &Self::Block,
    ) -> impl Future<Output = Result<()>> + Send;
}
