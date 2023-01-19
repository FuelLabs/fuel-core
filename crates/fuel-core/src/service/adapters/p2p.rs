#[cfg(feature = "p2p")]
use super::BlockHeightImportAdapter;
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
use tokio::sync::broadcast::Sender;

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        self.get_sealed_block_by_height(height)
    }
}
#[cfg(feature = "p2p")]
impl BlockHeightImportAdapter {
    pub fn new(blocks: Sender<BlockHeight>) -> Self {
        Self { blocks }
    }
}
#[cfg(feature = "p2p")]
impl BlockHeightImporter for BlockHeightImportAdapter {
    fn next_block_height(&self) -> BoxStream<BlockHeight> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.blocks.subscribe())
                .filter_map(|result| result.ok()),
        )
    }
}
