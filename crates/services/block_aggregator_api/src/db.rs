use crate::{
    blocks::Block,
    result::Result,
};

pub trait BlockAggregatorDB: Send + Sync {
    type BlockRange;

    fn store_block(
        &mut self,
        id: u64,
        block: Block,
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_block_range(
        &self,
        first: u64,
        last: u64,
    ) -> impl Future<Output = Result<Self::BlockRange>> + Send;

    fn get_current_height(&self) -> impl Future<Output = Result<u64>> + Send;
}
