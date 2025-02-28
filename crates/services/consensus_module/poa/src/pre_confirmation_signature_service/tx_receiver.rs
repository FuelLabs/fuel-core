use super::*;
use std::future::Future;

/// Receives a set of transactions from the block producer. The state of these transactions is
/// assumed to be final, as the block producer has the final call.
pub trait TxReceiver: Send {
    type Txs: Send;

    type Sender: TxSender<Txs = Self::Txs>;
    fn receive(&mut self) -> impl Future<Output = Result<Self::Txs>> + Send;

    fn get_sender(&self) -> Result<Self::Sender>;
}

pub trait TxSender: Send {
    type Txs: Send;
    fn send(&mut self, txs: Self::Txs) -> impl Future<Output = Result<()>> + Send;
}
