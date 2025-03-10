use thiserror::Error;

/// Errors that can occur during compression
#[derive(Error, Debug)]
pub enum CompressionError {
    // L2 block source Errors
    /// Failed to get l2 block
    #[error("failed to get l2 block: `{0}`")]
    FailedToGetBlock(String),
    // Compression storage Errors
    /// Failed to read compressed block from storage
    #[error("failed to write compressed block to storage: `{0}`")]
    FailedToWriteCompressedBlock(String),
    // Service errors
    /// Failed to create service
    #[error("failed to create service: `{0}`")]
    FailedToCreateService(String),
    /// Failed to handle new block
    #[error("failed to handle new block: `{0}`")]
    FailedToHandleNewBlock(String),
}
