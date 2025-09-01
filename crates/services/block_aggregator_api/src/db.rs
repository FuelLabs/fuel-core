use crate::{
    blocks::Block,
    result::{
        Error,
        Result,
    },
};
use fuel_core_services::stream::BoxStream;

pub trait BlockAggregatorDB: Send + Sync {
    fn store_block(&mut self, block: Block) -> Result<()>;
    fn get_block_range(&self, first: u64, last: u64) -> Result<BoxStream<Block>>;
}
