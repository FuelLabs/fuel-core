use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that can occur during the pre-confirmation process.
#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum Error {
    /// Error occurred when broadcasting data
    #[error("Broadcast error: {0}")]
    Broadcast(String),

    /// Error occurred when receiving transactions
    #[error("Transaction receiver error: {0}")]
    TxReceiver(String),

    /// Error occurred inside of trigger
    #[error("Trigger error: {0}")]
    Trigger(String),

    /// Error occurred with parent signature
    #[error("Parent signature error: {0}")]
    ParentSignature(String),
}
