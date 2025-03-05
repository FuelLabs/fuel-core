use super::*;
use crate::pre_confirmation_signature_service::signing_key::SigningKey;
use std::future::Future;

pub struct ExpiringKey<K> {
    pub key: K,
    pub expiration: Tai64,
}

impl<K> ExpiringKey<K> {
    pub fn new(key: K, expiration: Tai64) -> Self {
        Self { key, expiration }
    }

    pub fn expiration(&self) -> Tai64 {
        self.expiration
    }
}

impl<K: SigningKey> ExpiringKey<K> {
    pub fn public_key(&self) -> K::PublicKey {
        self.key.public_key()
    }

    pub fn sign<T>(&self, data: &T) -> Result<K::Signature>
    where
        T: Serialize,
    {
        self.key.sign(data)
    }
}

/// Defines the mechanism for generating new delegate keys
pub trait KeyGenerator: Send {
    type Key: SigningKey;
    fn generate(
        &mut self,
        expiration: Tai64,
    ) -> impl Future<Output = ExpiringKey<Self::Key>> + Send;
}
