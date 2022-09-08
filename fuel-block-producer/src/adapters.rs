use crate::ports::TxPool;
use fuel_core_interfaces::{
    common::fuel_tx::Transaction,
    txpool::Sender,
};
use std::sync::Arc;

#[async_trait::async_trait]
impl TxPool for Sender {
    async fn get_includable_txs(&self) -> anyhow::Result<Vec<Arc<Transaction>>> {
        self.includable().await
    }
}
