use crate::{
    adapters::transaction_selector::select_transactions,
    ports::TxPool,
};
use fuel_core_interfaces::{
    common::fuel_tx::{
        CheckedTransaction,
        ConsensusParameters,
    },
    model::BlockHeight,
    txpool::Sender,
};
use std::sync::Arc;

pub mod transaction_selector;

pub struct TxPoolAdapter {
    pub sender: Sender,
    pub consensus_params: ConsensusParameters,
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    async fn get_includable_txs(
        &self,
        block_height: BlockHeight,
        max_gas: u64,
    ) -> anyhow::Result<Vec<Arc<CheckedTransaction>>> {
        let includable_txs = self.sender.includable().await?;
        // TODO: The transaction pool should return transactions that are already checked
        let includable_txs = includable_txs
            .into_iter()
            .map(|tx| {
                CheckedTransaction::check_unsigned(
                    (*tx).clone(),
                    block_height.into(),
                    &self.consensus_params,
                )
                .map(Arc::new)
            })
            .collect::<Result<_, _>>()?;

        let includable_txs = select_transactions(includable_txs, max_gas);

        Ok(includable_txs)
    }
}
