use super::*;
use std::future::Future;

/// Used to sign the delegate keys, proving that the parent key approves of the delegation
pub trait ParentSignature: Send + Sync {
    type Signature: Clone + serde::Serialize + Send + Sync;

    fn sign<T>(&self, data: &T) -> impl Future<Output = Result<Self::Signature>> + Send
    where
        T: Serialize + Send + Sync;
}
