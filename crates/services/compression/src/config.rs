use std::time::Duration;

/// Compression configuration
#[derive(Debug, Clone, Copy)]
pub struct CompressionConfig {
    temporal_registry_retention: Duration,
}

impl CompressionConfig {
    /// Create a new compression configuration
    pub fn new(temporal_registry_retention: Duration) -> Self {
        Self {
            temporal_registry_retention,
        }
    }

    /// Get the temporal registry retention
    pub fn temporal_registry_retention(&self) -> Duration {
        self.temporal_registry_retention
    }
}

impl From<CompressionConfig> for fuel_core_compression::Config {
    fn from(config: CompressionConfig) -> Self {
        Self {
            temporal_registry_retention: config.temporal_registry_retention(),
        }
    }
}
