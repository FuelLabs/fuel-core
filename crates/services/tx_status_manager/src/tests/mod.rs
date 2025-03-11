#![allow(non_snake_case)]

use crate::service::SignatureVerification;
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
use tokio::sync::mpsc;

mod mocks;
mod tests_e2e;
mod tests_permits;
mod tests_sending;
mod tests_service;
mod tests_subscribe;
mod tests_update_stream_state;
mod universe;
mod utils;

pub(crate) struct FakeSignatureVerification {
    new_delegate_sender: mpsc::Sender<(DelegatePublicKey, ProtocolSignature)>,
    new_delegate_response: bool,
    preconfirmation_signature_success: bool,
}

impl FakeSignatureVerification {
    pub fn new_with_handles(
        new_delegate_response: bool,
        preconfirmation_signature_success: bool,
    ) -> (Self, mpsc::Receiver<(DelegatePublicKey, ProtocolSignature)>) {
        let (new_delegate_sender, new_delegate_receiver) = mpsc::channel(1_000);
        let adapter = Self {
            new_delegate_sender,
            new_delegate_response,
            preconfirmation_signature_success,
        };
        (adapter, new_delegate_receiver)
    }
}

impl SignatureVerification for FakeSignatureVerification {
    async fn add_new_delegate(
        &mut self,
        _delegate: DelegatePublicKey,
        _protocol_signature: ProtocolSignature,
    ) -> bool {
        tracing::debug!("FakeSignatureVerification::add_new_delegate");
        self.new_delegate_sender
            .send((_delegate, _protocol_signature))
            .await
            .unwrap();
        self.new_delegate_response
    }

    async fn check_preconfirmation_signature(
        &mut self,
        _sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        self.preconfirmation_signature_success
    }
}
