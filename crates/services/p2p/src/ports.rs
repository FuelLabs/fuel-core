use std::sync::Arc;

use async_trait::async_trait;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};

#[async_trait]
pub trait Database: Send + Sync {
    async fn get_sealed_block(
        &self,
        block_height: BlockHeight,
    ) -> Option<Arc<SealedBlock>>;
}
