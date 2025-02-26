use super::*;
use std::future::Future;

/// Used to sign the delegate keys, proving that the parent key approves of the delegation
pub trait ParentSignature: Send {
    type SignedData<T>: Send
    where
        T: Send;
    fn sign<T: Send>(
        &self,
        data: T,
    ) -> impl Future<Output = Result<Self::SignedData<T>>> + Send;
}
