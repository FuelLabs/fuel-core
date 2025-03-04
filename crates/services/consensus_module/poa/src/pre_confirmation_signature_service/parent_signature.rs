use super::*;
use std::future::Future;

// TODO(#2739): Remove when integrated
// link: https://github.com/FuelLabs/fuel-core/issues/2739
#[allow(dead_code)]
/// Used to sign the delegate keys, proving that the parent key approves of the delegation
pub trait ParentSignature<T: Send>: Send {
    type SignedData: Send;
    fn sign(&self, data: T) -> impl Future<Output = Result<Self::SignedData>> + Send;
}
