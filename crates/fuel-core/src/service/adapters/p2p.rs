#[cfg(feature = "p2p")]
use super::BlockHeightImportAdapter;
#[cfg(feature = "p2p")]
use crate::database::Database;
#[cfg(feature = "p2p")]
use fuel_core_p2p::ports::{
    BlockHeightImporter,
    P2pDb,
};
#[cfg(feature = "p2p")]
use fuel_core_services::stream::BoxStream;
#[cfg(feature = "p2p")]
use fuel_core_storage::Result as StorageResult;
#[cfg(feature = "p2p")]
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};
#[cfg(feature = "p2p")]
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
