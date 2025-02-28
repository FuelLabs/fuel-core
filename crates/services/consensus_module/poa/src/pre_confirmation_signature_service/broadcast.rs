use super::*;
use std::future::Future;

/// Broadcasts the delegate key as well as the pre-confirmed transactions to the network
pub trait Broadcast: Send {
    type PreConfirmations: Send + Clone;
    type ParentSignature<T>;
    type DelegateKey: SigningKey;

    fn broadcast_txs(
        &mut self,
        txs: Signed<Self::DelegateKey, Self::PreConfirmations>,
    ) -> impl Future<Output = Result<()>> + Send;

    fn broadcast_delegate_key(
        &mut self,
        delegate_key: Self::ParentSignature<Self::DelegateKey>,
    ) -> impl Future<Output = Result<()>> + Send;
}
