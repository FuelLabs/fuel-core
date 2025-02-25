use super::*;
use std::future::Future;

pub trait TxReceiver: Send {
    type Txs: Send;
    fn receive(&mut self) -> impl Future<Output = Result<Self::Txs>> + Send;
}
