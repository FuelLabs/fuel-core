use crate::config::CompressionConfig;
use fuel_core_types::fuel_types::ChainId;

/// Configuration for the compression service
pub trait CompressionConfigProvider: Send + Sync {
    /// getter for the compression config
    fn config(&self, chain_id: ChainId) -> CompressionConfig;
}
