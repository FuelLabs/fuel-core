use fuel_core_types::fuel_tx::{
    self,
    TxId,
    UtxoId,
};

/// Errors that can occur in the merkleized storage.
#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum Error {
    /// Errors indicating that the block is invalid.
    #[from]
    InvalidBlock(InvalidBlock),
    /// Errors originating from storage operations.
    #[from]
    Storage(fuel_core_storage::Error),
    /// Errors that can only occur if there is a bug in the code.
    #[from]
    InvariantViolation(InvariantViolation),
}

/// Error indicating that a block is invalid
#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum InvalidBlock {
    /// The block has too many transactions.
    TooManyTransactions,
    /// A transaction has too many outputs.
    TooManyOutputs,
    /// An output contains an invalid contract input index.
    InvalidContractInputIndex(u16),
    /// A transaction tries to increment the consensus parameter version beyond the supported range.
    ConsensusParametersVersionOutOfBounds,
    /// A transaction tries to increment the state transition bytecode version beyond the supported range.
    StateTransitionBytecodeVersionOutOfBounds,
    /// A transaction already exists.
    DuplicateTransaction(TxId),
    /// A transaction is missing a referenced witness.
    #[display(fmt = "missing witness at index {witness_index} in tx {txid}")]
    MissingWitness {
        /// Transaction ID
        txid: TxId,
        /// Index of missing witness
        witness_index: usize,
    },
    /// A create transaction is missing a contract created output.
    ContractCreatedOutputNotFound,
    /// An upload transaction tries to upload code that has already been completely uploaded.
    BytecodeAlreadyCompleted,
    /// An upload transaction tries to upload the wrong subsection.
    #[display(fmt = "wrong subsection index: expected {expected}, got {observed}")]
    WrongSubsectionIndex {
        /// The expected subsection.
        expected: u16,
        /// The observed subsection.
        observed: u16,
    },
    /// An upload transaction tries to upload a subsection index outside of the supported range.
    SubsectionNumberOverflow,
    /// A transaction doesn't satisfy the transaction validity rules
    #[from]
    TxValidityError(fuel_tx::ValidityError),
}

/// Error indicating that there is a bug in the code.
#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum InvariantViolation {
    /// We try to store a coin that already exists.
    CoinAlreadyExists(UtxoId),
}

impl core::error::Error for Error {}
impl core::error::Error for InvalidBlock {}

/// Convenience alias for merkleized storage results
pub type Result<T> = core::result::Result<T, Error>;
