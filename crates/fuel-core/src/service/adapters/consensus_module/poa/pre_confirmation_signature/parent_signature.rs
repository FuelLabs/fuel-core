use crate::service::adapters::consensus_module::poa::pre_confirmation_signature::{
    key_generator::Ed25519Key,
};
use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    parent_signature::ParentSignature,
};
use fuel_core_types::fuel_crypto;
use fuel_core_types::signer::SignMode;

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

impl ParentSignature<Ed25519Key> for FuelParentSigner<Ed25519Key> {
    type SignedData = FuelParentSignature<Ed25519Key>;

    async fn sign(&self, data: Ed25519Key) -> PoAResult<Self::SignedData> {
        let message = data.into();
        let signature = self.mode.sign_message(message).await.map_err(|e| {
            PoaError::ParentSignature(format!("Failed to sign message: {}", e))
        })?;
        Ok(signature.into())
    }
}

impl From<Ed25519Key> for fuel_crypto::Message {
    fn from(_value: Ed25519Key) -> Self {
        todo!()
    }
}
