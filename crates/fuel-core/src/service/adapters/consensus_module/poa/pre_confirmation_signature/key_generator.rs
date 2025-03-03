use ed25519::signature::Signer;
use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoAError,
        Result as PoAResult,
    },
    key_generator::KeyGenerator,
    signing_key::SigningKey,
};

pub struct Ed25519KeyGenerator;

impl KeyGenerator for Ed25519KeyGenerator {
    type Key = Ed25519Key;

    async fn generate(&mut self) -> PoAResult<Self::Key> {
        todo!()
    }
}

use ed25519_dalek::SigningKey as DalekSigningKey;
use serde::Serialize;

#[derive(Clone)]
pub struct Ed25519Key {
    signer: DalekSigningKey,
}

#[derive(Clone)]
pub struct Ed25519Signature<T> {
    _phantom: std::marker::PhantomData<T>,
    signature: ed25519::Signature,
}

impl<T> Ed25519Signature<T> {
    pub fn signature(&self) -> ed25519::Signature {
        self.signature
    }
}

impl<T> From<ed25519::Signature> for Ed25519Signature<T> {
    fn from(signature: ed25519::Signature) -> Self {
        Ed25519Signature {
            _phantom: std::marker::PhantomData,
            signature,
        }
    }
}

impl SigningKey for Ed25519Key {
    type Signature<T: Send + Clone + Serialize> = Ed25519Signature<T>;

    fn sign<T: Send + Clone + Serialize>(
        &self,
        data: T,
    ) -> PoAResult<Self::Signature<T>> {
        let bytes = postcard::to_allocvec(&data)
            .map_err(|e| PoAError::Signature(format!("{e:?}")))?;
        let signature = self.signer.sign(&bytes);
        Ok(signature.into())
    }
}
