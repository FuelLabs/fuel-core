use ed25519::signature::Signer;
use ed25519_dalek::SigningKey as DalekSigningKey;
use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoAError,
        Result as PoAResult,
    },
    key_generator::KeyGenerator,
    signing_key::SigningKey,
};
use fuel_core_types::fuel_crypto::SecretKey;
use rand::{
    prelude::StdRng,
    SeedableRng,
};
use serde::Serialize;
use std::ops::Deref;

pub struct Ed25519KeyGenerator;

impl KeyGenerator for Ed25519KeyGenerator {
    type Key = Ed25519Key;

    async fn generate(&mut self) -> PoAResult<Self::Key> {
        let mut rng = StdRng::from_entropy();
        let secret = SecretKey::random(&mut rng);

        Ok(Ed25519Key {
            signer: DalekSigningKey::from_bytes(secret.deref()),
        })
    }
}

#[derive(Clone)]
pub struct Ed25519Key {
    signer: DalekSigningKey,
}

impl SigningKey for Ed25519Key {
    type Signature = ed25519::Signature;
    type PublicKey = ed25519_dalek::VerifyingKey;

    fn public_key(&self) -> Self::PublicKey {
        self.signer.verifying_key()
    }

    fn sign<T>(&self, data: &T) -> PoAResult<Self::Signature>
    where
        T: Serialize,
    {
        let bytes = postcard::to_allocvec(data)
            .map_err(|e| PoAError::Signature(format!("{e:?}")))?;
        let signature = self.signer.sign(&bytes);
        Ok(signature)
    }
}
