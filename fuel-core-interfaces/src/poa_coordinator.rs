use crate::{
    common::fuel_tx::TxId,
    model::{
        ArcPoolTx,
        BlockHeight,
    },
};
use anyhow::Result;

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> Result<BlockHeight>;
}

#[async_trait::async_trait]
pub trait TransactionPool {
    /// Returns the number of pending transactions in the `TxPool`.
    async fn pending_number(&self) -> Result<usize>;

    async fn total_consumable_gas(&self) -> Result<u64>;

    async fn remove_txs(&mut self, tx_ids: Vec<TxId>) -> Result<Vec<ArcPoolTx>>;
}
