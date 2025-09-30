use crate::{
    block_range_response::BlockRangeResponse,
    blocks::Block,
    db::BlockStorage,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct RemoteCache;

impl BlockStorage for RemoteCache {
    type BlockRangeResponse = BlockRangeResponse;

    fn store_block(
        &mut self,
        height: BlockHeight,
        block: Block,
    ) -> impl Future<Output = crate::result::Result<()>> + Send {
        todo!()
    }

    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> impl Future<Output = crate::result::Result<Self::BlockRangeResponse>> + Send
    {
        todo!()
    }

    fn get_current_height(
        &self,
    ) -> impl Future<Output = crate::result::Result<BlockHeight>> + Send {
        todo!()
    }
}
