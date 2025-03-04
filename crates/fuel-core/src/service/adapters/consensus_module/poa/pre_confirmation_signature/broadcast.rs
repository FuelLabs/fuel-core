use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::{
    key_generator::Ed25519Key,
    parent_signature::FuelParentSigner,
};
use fuel_core_poa::pre_confirmation_signature_service::{
    broadcast::{
        Broadcast,
        PublicKey,
        Signature,
    },
    error::Result as PoAResult,
    parent_signature::ParentSignature,
};
use fuel_core_types::services::p2p::{
    DelegatePreConfirmationKey,
    Preconfirmation,
};

/// TODO: Implement `Broadcast` properly: <https://github.com/FuelLabs/fuel-core/issues/2783>
pub struct P2PBroadcast;

impl Broadcast for P2PBroadcast {
    type DelegateKey = Ed25519Key;
    type ParentKey = FuelParentSigner;
    type Preconfirmations = Vec<Preconfirmation>;

    async fn broadcast_preconfirmations(
        &mut self,
        _: Self::Preconfirmations,
        _: Signature<Self>,
    ) -> PoAResult<()> {
        todo!()
    }

    async fn broadcast_delegate_key(
        &mut self,
        _: DelegatePreConfirmationKey<PublicKey<Self>>,
        _: <Self::ParentKey as ParentSignature>::Signature,
    ) -> PoAResult<()> {
        todo!()
    }
}
