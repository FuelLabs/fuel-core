use fuel_core_poa::pre_confirmation_signature_service::{
    error::{
        Error as PoAError,
        Result as PoAResult,
    },
    key_generator::{
        ExpiringKey,
        KeyGenerator,
    },
    signing_key::SigningKey,
};
use fuel_core_types::{
    ed25519::signature::Signer,
    ed25519_dalek::SigningKey as DalekSigningKey,
    fuel_crypto::SecretKey,
    tai64::Tai64,
};
use rand::{
    SeedableRng,
    prelude::StdRng,
};
use serde::Serialize;
use std::ops::Deref;

pub struct Ed25519KeyGenerator;

impl KeyGenerator for Ed25519KeyGenerator {
    type Key = Ed25519Key;

    async fn generate(&mut self, expiration: Tai64) -> ExpiringKey<Self::Key> {
        let mut rng = StdRng::from_entropy();
        let secret = SecretKey::random(&mut rng);
        let key = Ed25519Key {
            signer: DalekSigningKey::from_bytes(secret.deref()),
        };
        ExpiringKey::new(key, expiration)
    }
}

#[derive(Clone)]
pub struct Ed25519Key {
    signer: DalekSigningKey,
}

impl SigningKey for Ed25519Key {
    type Signature = fuel_core_types::ed25519::Signature;
    type PublicKey = fuel_core_types::ed25519_dalek::VerifyingKey;

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
