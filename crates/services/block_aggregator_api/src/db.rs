use crate::{
    blocks::Block,
    result::Result,
};
use fuel_core_services::stream::BoxStream;

pub trait BlockAggregatorDB: Send + Sync {
    fn store_block(
        &mut self,
        id: u64,
        block: Block,
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_block_range(
        &self,
        first: u64,
        last: u64,
    ) -> impl Future<Output = Result<BoxStream<Block>>> + Send;
}
