use super::*;

#[async_trait::async_trait]
pub trait ParentSignature: Send {
    type SignedData<T>: Send
    where
        T: Send;
    async fn sign<T: Send>(&self, data: T) -> Result<Self::SignedData<T>>;
}
