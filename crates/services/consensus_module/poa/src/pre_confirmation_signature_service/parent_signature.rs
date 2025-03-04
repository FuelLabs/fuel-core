use super::*;
use std::future::Future;

// TODO(#2739): Remove when integrated
// link: https://github.com/FuelLabs/fuel-core/issues/2739
#[allow(dead_code)]
/// Used to sign the delegate keys, proving that the parent key approves of the delegation
pub trait ParentSignature: Send {
    type Signature: serde::Serialize;

    fn sign<T>(&self, data: &T) -> impl Future<Output = Result<Self::Signature>> + Send
    where
        T: Serialize + Send + Sync;
}
