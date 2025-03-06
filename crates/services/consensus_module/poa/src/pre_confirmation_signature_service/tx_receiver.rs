use std::future::Future;

use super::*;

/// Receives a set of transactions from the block producer. The state of these transactions is
/// assumed to be final, as the block producer has the final call.
pub trait TxReceiver: Send {
    type Txs: serde::Serialize + Send;

    fn receive(&mut self) -> impl Future<Output = Result<Self::Txs>> + Send;
}
