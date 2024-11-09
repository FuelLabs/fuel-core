//! Ports used by the relayer to access the outside world

use cosmrs::crypto::PublicKey;

// pub trait SharedSequencerClient: Send + Sync {
//     fn send(
//         &mut self,
//         signing_key: &Secret<SecretKeyWrapper>,
//         block: SealedBlock,
//     ) -> anyhow::Result<()>;
// }

/// A signer that can sign arbitrary data
#[async_trait::async_trait]
pub trait Signer: Send + Sync {
    /// Sign data using a key
    async fn sign(&self, data: &[u8]) -> anyhow::Result<Vec<u8>>;
    /// Get the public key of the signer. Panics if the key is not available.
    fn public_key(&self) -> PublicKey;
    /// Check if the signer is available
    fn is_available(&self) -> bool;
}
