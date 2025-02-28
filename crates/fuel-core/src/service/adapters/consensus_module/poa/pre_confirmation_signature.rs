use fuel_core_poa::pre_confirmation_signature_service::signing_key::SigningKey;
use fuel_core_types::{
    fuel_tx::TxId,
    services::p2p::PreconfirmationStatus,
};

pub mod broadcast;
pub mod key_generator;
pub mod parent_signature;
pub mod trigger;
pub mod tx_receiver;

pub type Preconfirmations = Vec<(TxId, PreconfirmationStatus)>;

#[derive(Clone)]
pub struct DummyKey;

#[derive(Clone)]
pub struct DummyKeySignature<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl SigningKey for DummyKey {
    type Signature<T: Send + Clone> = DummyKeySignature<T>;

    fn sign<T: Send + Clone>(
        &self,
        _data: T,
    ) -> fuel_core_poa::pre_confirmation_signature_service::error::Result<
        Self::Signature<T>,
    > {
        todo!()
    }
}
