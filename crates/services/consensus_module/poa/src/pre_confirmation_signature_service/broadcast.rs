use super::Result;
use crate::pre_confirmation_signature_service::{
    parent_signature::ParentSignature,
    signing_key::SigningKey,
};
use core::future::Future;
use fuel_core_types::services::p2p::DelegatePreConfirmationKey;

#[allow(type_alias_bounds)]
pub type PublicKey<S>
where
    S: Broadcast,
= <S::DelegateKey as SigningKey>::PublicKey;

#[allow(type_alias_bounds)]
pub type Signature<S>
where
    S: Broadcast,
= <S::DelegateKey as SigningKey>::Signature;

/// Broadcasts the delegate key as well as the pre-confirmed transactions to the network
pub trait Broadcast: Send {
    type ParentKey: ParentSignature;
    type DelegateKey: SigningKey;
    type Preconfirmations;

    fn broadcast_preconfirmations(
        &mut self,
        message: Self::Preconfirmations,
        signature: Signature<Self>,
    ) -> impl Future<Output = Result<()>> + Send;

    fn broadcast_delegate_key(
        &mut self,
        delegate: DelegatePreConfirmationKey<PublicKey<Self>>,
        nonce: u64,
        signature: <Self::ParentKey as ParentSignature>::Signature,
    ) -> impl Future<Output = Result<()>> + Send;
}
