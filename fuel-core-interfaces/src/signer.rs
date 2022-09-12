use crate::common::fuel_types::Bytes32;
use async_trait::async_trait;
use thiserror::Error;

/// Dummy signer that will be removed in next pull request.
/// TODO do not use.
#[async_trait]
pub trait Signer {
    async fn sign(&self, hash: &Bytes32) -> Result<Bytes32, SignerError>;
}

#[derive(Error, Debug)]
pub enum SignerError {
    #[error("Private key not loaded")]
    KeyNotLoaded,
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod helpers {
    use super::*;

    pub struct DummySigner {}

    #[async_trait]
    impl Signer for DummySigner {
        async fn sign(&self, hash: &Bytes32) -> Result<Bytes32, SignerError> {
            Ok(*hash)
        }
    }
}
