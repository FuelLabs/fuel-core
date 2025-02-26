use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that can occur during the pre-confirmation process.
#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum Error {
    /// Error occurred when broadcasting data
    #[error("Broadcast error: {0}")]
    BroadcastError(String),

    /// Error occurred when receiving transactions
    #[error("Transaction receiver error: {0}")]
    TxReceiverError(String),
}
