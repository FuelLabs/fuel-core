use async_trait::async_trait;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};

#[async_trait]
pub trait P2pDb: Send + Sync {
    async fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>>;
}
