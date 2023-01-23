use crate::database::Database;
use fuel_core_p2p::ports::P2pDb;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        self.get_sealed_block_by_height(height)
    }
}
