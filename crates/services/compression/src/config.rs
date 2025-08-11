use fuel_core_types::fuel_types::ChainId;
use std::{
    num::NonZeroU32,
    time::Duration,
};

/// Compression configuration
#[derive(Debug, Clone, Copy)]
pub struct CompressionConfig {
    temporal_registry_retention: Duration,
    starting_height: Option<NonZeroU32>,
    metrics: bool,
    chain_id: ChainId,
}

impl CompressionConfig {
    /// Create a new compression configuration
    pub fn new(
        temporal_registry_retention: Duration,
        starting_height: Option<NonZeroU32>,
        metrics: bool,
        chain_id: ChainId,
    ) -> Self {
        Self {
            temporal_registry_retention,
            starting_height,
            metrics,
            chain_id,
        }
    }

    /// Get the temporal registry retention
    pub fn temporal_registry_retention(&self) -> Duration {
        self.temporal_registry_retention
    }

    /// Get the metrics configuration
    pub fn metrics(&self) -> bool {
        self.metrics
    }

    /// Get the override starting height
    pub fn starting_height(&self) -> Option<u32> {
        self.starting_height.map(|height| height.get())
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> ChainId {
        self.chain_id
    }
}

impl From<&CompressionConfig> for fuel_core_compression::Config {
    fn from(config: &CompressionConfig) -> Self {
        Self {
            temporal_registry_retention: config.temporal_registry_retention(),
        }
    }
}
