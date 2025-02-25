use super::*;
use std::future::Future;

pub trait ParentSignature: Send {
    type SignedData<T>: Send
    where
        T: Send;
    fn sign<T: Send>(
        &self,
        data: T,
    ) -> impl Future<Output = Result<Self::SignedData<T>>> + Send;
}
