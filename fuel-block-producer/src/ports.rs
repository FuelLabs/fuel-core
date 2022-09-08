use anyhow::Result;
use async_trait::async_trait;
use fuel_core_interfaces::{
    common::{
        fuel_tx::Transaction,
        fuel_types::Address,
    },
    model::DaBlockHeight,
};
use std::sync::Arc;

#[async_trait]
pub trait Relayer: Sync + Send {
    /// Get the block production key associated with a given
    /// validator id as of a specific block height
    async fn get_block_production_key(
        &self,
        validator_id: Address,
        da_height: DaBlockHeight,
    ) -> Result<Address>;

    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight>;
}

#[async_trait]
pub trait TxPool: Sync + Send {
    async fn get_includable_txs(&self) -> Result<Vec<Arc<Transaction>>>;
}
