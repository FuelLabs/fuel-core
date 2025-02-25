use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that can occur during the pre-confirmation process.
#[derive(Error, Debug)]
pub enum Error {
    // /// Error that occurs when the parent signature is invalid.
    // #[error("Invalid parent signature: {0}")]
    // InvalidParentSignature(String),
    /// Error occurred when broadcasting data
    #[error("Broadcast error: {0}")]
    BroadcastError(String),

    /// Error occurred when receiving transactions
    #[error("Transaction receiver error: {0}")]
    TxReceiverError(String),
}
