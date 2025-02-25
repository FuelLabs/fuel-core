use super::*;

#[async_trait::async_trait]
pub trait Broadcast: Send {
    type PreConfirmations: Send + Clone;
    type ParentSignature<T>;
    type DelegateKey: SigningKey;

    async fn broadcast_txs(
        &mut self,
        txs: <Self::DelegateKey as SigningKey>::Signature<Self::PreConfirmations>,
    ) -> Result<()>;

    async fn broadcast_delegate_key(
        &mut self,
        delegate_key: Self::ParentSignature<Self::DelegateKey>,
    ) -> Result<()>;
}
