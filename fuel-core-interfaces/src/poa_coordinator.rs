use anyhow::Result;
use fuel_vm::fuel_tx::Bytes32;

use crate::model::{
    BlockHeight,
    FuelBlockConsensus,
};

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;

    // Returns error if already sealed
    fn seal_block(
        &mut self,
        block_id: Bytes32,
        consensus: FuelBlockConsensus,
    ) -> Result<()>;
}

#[async_trait::async_trait]
pub trait TransactionPool {
    async fn total_consumable_gas(&self) -> Result<u64>;
}
