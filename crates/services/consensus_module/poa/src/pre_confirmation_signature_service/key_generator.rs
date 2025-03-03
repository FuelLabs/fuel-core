use super::*;
use crate::pre_confirmation_signature_service::signing_key::SigningKey;
use std::future::Future;

/// Defines the mechanism for generating new delegate keys
pub trait KeyGenerator: Send {
    type Key: SigningKey;
    fn generate(&mut self) -> impl Future<Output = Result<Self::Key>> + Send;
}
