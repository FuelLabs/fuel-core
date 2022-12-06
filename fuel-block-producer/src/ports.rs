use async_trait::async_trait;
use fuel_core_interfaces::model::{
    ArcPoolTx,
    BlockHeight,
    DaBlockHeight,
};

#[async_trait]
pub trait TxPool: Sync + Send {
    async fn get_includable_txs(
        &self,
        // could be used by the txpool to filter txs based on maturity
        block_height: BlockHeight,
        // The upper limit for the total amount of gas of these txs
        max_gas: u64,
    ) -> anyhow::Result<Vec<ArcPoolTx>>;
}

#[async_trait::async_trait]
pub trait Relayer: Sync + Send {
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> anyhow::Result<DaBlockHeight>;
}
