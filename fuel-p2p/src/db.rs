use fuel_core_interfaces::model::{
    BlockHeight,
    SealedFuelBlock,
};
use fuel_database::Database;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait P2pDb: Send + Sync {
    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;
}

#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        let block_id = self
            .get_block_id(height)
            .unwrap_or_else(|_| panic!("nonexistent block height {}", height))?;

        self.get_sealed_block(&block_id)
            .expect("expected to find sealed block")
            .map(Arc::new)
    }
}
