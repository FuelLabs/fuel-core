use super::*;

#[async_trait::async_trait]
pub trait TxReceiver: Send {
    type Txs: Send;
    async fn receive(&mut self) -> Result<Self::Txs>;
}
