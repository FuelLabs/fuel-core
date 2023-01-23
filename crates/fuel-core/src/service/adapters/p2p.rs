use super::BlockImportAdapter;
use crate::database::Database;
use fuel_core_p2p::ports::{
    BlockHeightImporter,
    P2pDb,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};

#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        self.get_sealed_block_by_height(height)
    }
}

impl BlockHeightImporter for BlockImportAdapter {
    fn next_block_height(&self) -> BoxStream<BlockHeight> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.tx.subscribe())
                .filter_map(|result| result.ok())
                .map(|sealed_block| sealed_block.entity.header().consensus.height),
        )
    }
}
