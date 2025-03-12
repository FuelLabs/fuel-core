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
use std::sync::Arc;
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

use fuel_core_types::services::p2p::DelegatePreConfirmationKey;
use tracing_subscriber as _;

pub(crate) struct FakeSignatureVerification {
    new_delegate_sender: mpsc::Sender<
        Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    >,
    preconfirmation_signature_success: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) struct FakeSignatureVerificationHandles {
    pub new_delegate_receiver: mpsc::Receiver<
        Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    >,
    pub verification_result: Arc<std::sync::atomic::AtomicBool>,
}

impl FakeSignatureVerificationHandles {
    pub fn update_signature_verification_result(&mut self, result: bool) {
        self.verification_result
            .store(result, std::sync::atomic::Ordering::Relaxed);
    }
}

impl FakeSignatureVerification {
    pub fn new_with_handles(
        preconfirmation_signature_success: bool,
    ) -> (Self, FakeSignatureVerificationHandles) {
        let (new_delegate_sender, new_delegate_receiver) = mpsc::channel(1_000);
        let preconfirmation_signature_success = Arc::new(
            std::sync::atomic::AtomicBool::new(preconfirmation_signature_success),
        );
        let verification_result = preconfirmation_signature_success.clone();
        let adapter = Self {
            new_delegate_sender,
            preconfirmation_signature_success,
        };
        let handles = FakeSignatureVerificationHandles {
            new_delegate_receiver,
            verification_result,
        };
        (adapter, handles)
    }
}

impl SignatureVerification for FakeSignatureVerification {
    async fn add_new_delegate(
        &mut self,
        sealed: &Sealed<DelegatePreConfirmationKey<DelegatePublicKey>, ProtocolSignature>,
    ) -> bool {
        tracing::debug!("FakeSignatureVerification::add_new_delegate");
        self.new_delegate_sender.send(sealed.clone()).await.unwrap();
        true
    }

    async fn check_preconfirmation_signature(
        &mut self,
        _sealed: &Sealed<Preconfirmations, Bytes64>,
    ) -> bool {
        self.preconfirmation_signature_success
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}
