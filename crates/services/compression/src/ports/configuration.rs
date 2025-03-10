use crate::config::CompressionConfig;

/// Configuration for the compression service
pub trait CompressionConfigProvider {
    /// getter for the compression config
    fn config(&self) -> CompressionConfig;
}
