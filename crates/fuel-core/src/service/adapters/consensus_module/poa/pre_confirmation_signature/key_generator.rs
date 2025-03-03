use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::{
    signing_key::DummyKey,
};
use fuel_core_poa::pre_confirmation_signature_service::{
    error::Result as PoAResult,
    key_generator::KeyGenerator,
};

pub struct DelegateKeyGenerator;

impl KeyGenerator for DelegateKeyGenerator {
    type Key = DummyKey;

    async fn generate(&mut self) -> PoAResult<Self::Key> {
        todo!()
    }
}
