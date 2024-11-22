//! Ports used by the shared sequencer to access the outside world

use cosmrs::crypto::PublicKey;
use fuel_core_services::stream::BoxStream;
use fuel_core_types::services::block_importer::SharedImportResult;

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

/// Provider of the blocks.
pub trait BlocksProvider {
    /// Subscribe to new blocks.
    fn subscribe(&self) -> BoxStream<SharedImportResult>;
}
