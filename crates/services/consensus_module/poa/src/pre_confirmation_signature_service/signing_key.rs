use super::*;
use serde::Serialize;

/// Abstraction of the delegate signing key that can be used to sign data and produce a signature.
pub trait SigningKey: Send {
    type Signature: Clone + serde::Serialize;
    type PublicKey: Clone + serde::Serialize + Send + Sync;

    fn public_key(&self) -> Self::PublicKey;

    fn sign<T>(&self, data: &T) -> Result<Self::Signature>
    where
        T: Serialize;
}
