use super::*;
use std::future::Future;

/// Defines the behavior for when the `PreconfirmationSignatureTask` should rotate the delegate key
pub trait KeyRotationTrigger: Send {
    fn next_rotation(&mut self) -> impl Future<Output = Result<Tai64>> + Send;
}
