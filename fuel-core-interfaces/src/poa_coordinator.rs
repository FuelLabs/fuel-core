use anyhow::Result;

use crate::model::{
    BlockHeight,
    BlockId,
    FuelBlockConsensus,
};

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;

    // Returns error if already sealed
    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: FuelBlockConsensus,
    ) -> Result<()>;
}

#[async_trait::async_trait]
pub trait TransactionPool {
    async fn total_consumable_gas(&self) -> Result<u64>;
}
