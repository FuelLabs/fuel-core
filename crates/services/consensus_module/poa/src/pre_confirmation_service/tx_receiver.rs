use super::*;
use fuel_core_types::fuel_types::BlockHeight;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Success { height: BlockHeight },
    Fail { height: BlockHeight },
    SqueezedOut { reason: String },
}

#[async_trait::async_trait]
pub trait TxReceiver: Send {
    type Txs: Send;
    async fn receive(&mut self) -> Result<Self::Txs>;
}
