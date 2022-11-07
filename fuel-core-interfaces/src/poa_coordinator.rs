use crate::{
    common::fuel_tx::TxId,
    model::{
        ArcPoolTx,
        BlockHeight,
        BlockId,
        FuelBlockConsensus,
    },
};
use anyhow::Result;

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

    async fn remove_txs(&mut self, tx_ids: Vec<TxId>) -> Result<Vec<ArcPoolTx>>;
}
