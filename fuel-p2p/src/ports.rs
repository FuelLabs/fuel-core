use std::sync::Arc;

use async_trait::async_trait;
use fuel_core_interfaces::model::{
    BlockHeight,
    SealedFuelBlock,
};

#[async_trait]
pub trait Database: Send + Sync {
    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;
}
