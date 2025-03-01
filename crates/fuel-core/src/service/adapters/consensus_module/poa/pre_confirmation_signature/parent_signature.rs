use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    parent_signature::ParentSignature,
};
use fuel_core_types::{
    signer::SignMode,
};
use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::signing_key::DummyKey;

pub struct FuelParentSigner<T> {
    mode: SignMode,
    _phantom: std::marker::PhantomData<T>,
}

pub struct FuelParentSignature<T> {
    signature: fuel_core_types::fuel_vm::Signature,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> FuelParentSignature<T> {
    pub fn signature(&self) -> fuel_core_types::fuel_vm::Signature {
        self.signature
    }
}

impl<T> From<fuel_core_types::fuel_vm::Signature> for FuelParentSignature<T> {
    fn from(signature: fuel_core_types::fuel_vm::Signature) -> Self {
        Self {
            signature,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl ParentSignature<DummyKey> for FuelParentSigner<DummyKey> {
    type SignedData = FuelParentSignature<DummyKey>;

    async fn sign(&self, data: DummyKey) -> PoAResult<Self::SignedData> {
        let message = data.into();
        let signature = self.mode.sign_message(message).await.map_err(|e| {
            PoaError::ParentSignature(format!("Failed to sign message: {}", e))
        })?;
        Ok(signature.into())
    }
}
