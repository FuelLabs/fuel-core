use fuel_core_tx_status_manager::service::SignatureVerification;
use fuel_core_types::{
    fuel_tx::Bytes64,
    services::{
        p2p::{
            DelegatePublicKey,
            ProtocolSignature,
            Sealed,
        },
        preconfirmation::Preconfirmations,
    },
};

pub struct PreconfirmationSignatureVerification;

impl SignatureVerification for PreconfirmationSignatureVerification {
    async fn add_new_delegate(
        &mut self,
        _delegate: DelegatePublicKey,
        _protocol_signature: ProtocolSignature,
    ) -> bool {
        true
    }

    async fn check_preconfirmation_signature(
        &mut self,
        _sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        true
    }
}
