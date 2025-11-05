use crate::{
    block_range_response::BlockRangeResponse,
    blocks::Block,
    db::BlockAggregatorDB,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct RemoteCache;

impl BlockAggregatorDB for RemoteCache {
    type Block = Block;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(
        &mut self,
        height: BlockHeight,
        block: Block,
    ) -> crate::result::Result<()> {
        todo!()
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> crate::result::Result<Self::BlockRangeResponse> {
        todo!()
    }

    async fn get_current_height(&self) -> crate::result::Result<BlockHeight> {
        todo!()
    }
}
