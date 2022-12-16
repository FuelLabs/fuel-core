use crate::{
    adapters::transaction_selector::select_transactions,
    ports::TxPool,
};
use fuel_core_interfaces::txpool::Sender;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    services::txpool::{
        ArcPoolTx,
        Error as TxPoolError,
    },
};

pub mod transaction_selector;

pub struct TxPoolAdapter {
    pub sender: Sender,
}

#[async_trait::async_trait]
impl TxPool for TxPoolAdapter {
    async fn get_includable_txs(
        &self,
        _block_height: BlockHeight,
        max_gas: u64,
    ) -> Result<Vec<ArcPoolTx>, TxPoolError> {
        let includable_txs = select_transactions(
            self.sender
                .includable()
                .await
                .map_err(|e| TxPoolError::Other(e.to_string()))?,
            max_gas,
        );

        Ok(includable_txs)
    }
}
