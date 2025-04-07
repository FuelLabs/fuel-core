use std::time::Duration;

/// Compression configuration
#[derive(Debug, Clone, Copy)]
pub struct CompressionConfig {
    temporal_registry_retention: Duration,
    metrics: bool,
}

impl CompressionConfig {
    /// Create a new compression configuration
    pub fn new(temporal_registry_retention: Duration, metrics: bool) -> Self {
        Self {
            temporal_registry_retention,
            metrics,
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
}

impl From<CompressionConfig> for fuel_core_compression::Config {
    fn from(config: CompressionConfig) -> Self {
        Self {
            temporal_registry_retention: config.temporal_registry_retention(),
        }
    }
}
