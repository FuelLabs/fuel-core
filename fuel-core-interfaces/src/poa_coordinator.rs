use anyhow::Result;

use crate::model::BlockHeight;

pub trait BlockHeightDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;
}

#[async_trait::async_trait]
pub trait TransactionPool {
    async fn total_consumable_gas(&self) -> Result<u64>;
}
