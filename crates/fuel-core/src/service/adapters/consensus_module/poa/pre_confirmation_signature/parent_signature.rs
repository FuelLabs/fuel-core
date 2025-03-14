use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoaError,
        Result as PoAResult,
    },
    parent_signature::ParentSignature,
};
use fuel_core_types::{
    fuel_crypto,
    fuel_vm::Signature,
};
use serde::Serialize;

use crate::service::adapters::FuelBlockSigner;

impl ParentSignature for FuelBlockSigner {
    type Signature = Signature;

    async fn sign<T>(&self, data: &T) -> PoAResult<Self::Signature>
    where
        T: Serialize + Send + Sync,
    {
        let bytes = postcard::to_allocvec(data)
            .map_err(|e| PoaError::ParentSignature(format!("{e:?}")))?;
        let message = fuel_crypto::Message::new(bytes);
        let signature = self.mode.sign_message(message).await.map_err(|e| {
            PoaError::ParentSignature(format!("Failed to sign message: {}", e))
        })?;
        Ok(signature)
    }
}
