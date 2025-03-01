use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::{
    parent_signature::FuelParentSignature,
    signing_key::DummyKey,
    Preconfirmations,
};
use fuel_core_poa::pre_confirmation_signature_service::{
    broadcast::Broadcast,
    error::Result as PoAResult,
    Signed,
};

/// TODO: Implement `Broadcast` properly: <https://github.com/FuelLabs/fuel-core/issues/2783>
pub struct P2PBroadcast;

impl Broadcast for P2PBroadcast {
    type PreConfirmations = Preconfirmations;
    type ParentSignature = FuelParentSignature<DummyKey>;
    type DelegateKey = DummyKey;

    async fn broadcast_txs(
        &mut self,
        _txs: Signed<Self::DelegateKey, Self::PreConfirmations>,
    ) -> PoAResult<()> {
        todo!()
    }

    async fn broadcast_delegate_key(
        &mut self,
        _delegate_key: Self::ParentSignature,
    ) -> PoAResult<()> {
        todo!()
    }
}
