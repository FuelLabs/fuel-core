use fuel_core_interfaces::txpool::{
    Sender,
    TxPoolMpsc,
};
use fuel_core_poa::ports::TransactionPool;
use fuel_core_types::{
    fuel_tx::TxId,
    services::txpool::ArcPoolTx,
};
use tokio::sync::oneshot;

pub struct TxPoolAdapter {
    pub(crate) sender: Sender,
}

#[async_trait::async_trait]
impl TransactionPool for TxPoolAdapter {
    async fn pending_number(&self) -> anyhow::Result<usize> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(TxPoolMpsc::PendingNumber { response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    async fn total_consumable_gas(&self) -> anyhow::Result<u64> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(TxPoolMpsc::ConsumableGas { response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    async fn remove_txs(&mut self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(TxPoolMpsc::Remove { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }
}
