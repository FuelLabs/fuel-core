use super::*;

/// Abstraction of the delegate signing key that can be used to sign data and produce a signature.
pub trait SigningKey: Clone + Send {
    type Signature<T>: Clone + Send
    where
        T: Send + Clone;

    fn sign<T: Send + Clone>(&self, data: T) -> Result<Self::Signature<T>>;
}
