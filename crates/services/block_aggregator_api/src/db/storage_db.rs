use crate::{
    blocks::Block,
    db::BlockAggregatorDB,
};
use fuel_core_services::stream::BoxStream;

pub struct StorageDB;

impl BlockAggregatorDB for StorageDB {
    fn store_block(
        &mut self,
        id: u64,
        block: Block,
    ) -> impl Future<Output = crate::result::Result<()>> + Send {
        todo!()
    }

    fn get_block_range(
        &self,
        first: u64,
        last: u64,
    ) -> impl Future<Output = crate::result::Result<BoxStream<Block>>> + Send {
        todo!()
    }

    fn get_current_height(
        &self,
    ) -> impl Future<Output = crate::result::Result<u64>> + Send {
        todo!()
    }
}
