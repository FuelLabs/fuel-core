use super::*;
use std::future::Future;

/// Used to sign the delegate keys, proving that the parent key approves of the delegation
pub trait ParentSignature<T: Send>: Send {
    type SignedData: Send;
    fn sign(&self, data: T) -> impl Future<Output = Result<Self::SignedData>> + Send;
}
