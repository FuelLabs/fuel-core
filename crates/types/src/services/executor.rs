//! Types related to executor service.

use crate::{
    blockchain::{
        block::Block,
        header::{
            BlockHeaderError,
            ConsensusParametersVersion,
        },
    },
    entities::{
        coins::coin::Coin,
        relayer::{
            message::Message,
            transaction::RelayedTransactionId,
        },
    },
    fuel_tx::{
        Receipt,
        TxId,
        UtxoId,
        ValidityError,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
        Nonce,
    },
    fuel_vm::{
        checked_transaction::CheckError,
        ProgramState,
    },
    services::Uncommitted,
};

#[cfg(feature = "alloc")]
use alloc::{
    string::String,
    vec::Vec,
};

/// The alias for executor result.
pub type Result<T> = core::result::Result<T, Error>;
/// The uncommitted result of the block production execution.
pub type UncommittedResult<DatabaseTransaction> =
    Uncommitted<ExecutionResult, DatabaseTransaction>;

/// The uncommitted result of the block validation.
pub type UncommittedValidationResult<DatabaseTransaction> =
    Uncommitted<ValidationResult, DatabaseTransaction>;

/// The result of transactions execution for block production.
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug)]
pub struct ExecutionResult {
    /// Created block during the execution of transactions. It contains only valid transactions.
    pub block: Block,
    /// The list of skipped transactions with corresponding errors. Those transactions were
    /// not included in the block and didn't affect the state of the blockchain.
    pub skipped_transactions: Vec<(TxId, Error)>,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
    /// The list of all events generated during the execution of the block.
    pub events: Vec<Event>,
}

/// The result of the validation of the block.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug)]
pub struct ValidationResult {
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
    /// The list of all events generated during the execution of the block.
    pub events: Vec<Event>,
}

impl<DatabaseTransaction> UncommittedValidationResult<DatabaseTransaction> {
    /// Convert the `UncommittedValidationResult` into the `UncommittedResult`.
    pub fn into_execution_result(
        self,
        block: Block,
        skipped_transactions: Vec<(TxId, Error)>,
    ) -> UncommittedResult<DatabaseTransaction> {
        let Self {
            result: ValidationResult { tx_status, events },
            changes,
        } = self;
        UncommittedResult::new(
            ExecutionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            },
            changes,
        )
    }
}

impl<DatabaseTransaction> UncommittedResult<DatabaseTransaction> {
    /// Convert the `UncommittedResult` into the `UncommittedValidationResult`.
    pub fn into_validation_result(
        self,
    ) -> UncommittedValidationResult<DatabaseTransaction> {
        let Self {
            result: ExecutionResult {
                tx_status, events, ..
            },
            changes,
        } = self;
        UncommittedValidationResult::new(ValidationResult { tx_status, events }, changes)
    }
}

/// The event represents some internal state changes caused by the block execution.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Event {
    /// Imported a new spendable message from the relayer.
    MessageImported(Message),
    /// The message was consumed by the transaction.
    MessageConsumed(Message),
    /// Created a new spendable coin, produced by the transaction.
    CoinCreated(Coin),
    /// The coin was consumed by the transaction.
    CoinConsumed(Coin),
    /// Failed transaction inclusion
    ForcedTransactionFailed {
        /// The hash of the relayed transaction
        id: RelayedTransactionId,
        /// The height of the block that processed this transaction
        block_height: BlockHeight,
        /// The actual failure reason for why the forced transaction was not included
        failure: String,
    },
}

/// Known failure modes for processing forced transactions
#[derive(Debug, derive_more::Display)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ForcedTransactionFailure {
    /// Failed to decode transaction to a valid fuel_tx::Transaction
    #[display(fmt = "Failed to decode transaction")]
    CodecError,
    /// Transaction failed basic checks
    #[display(fmt = "Failed validity checks: {_0:?}")]
    CheckError(CheckError),
    /// Invalid transaction type
    #[display(fmt = "Transaction type is not accepted")]
    InvalidTransactionType,
    /// Execution error which failed to include
    #[display(fmt = "Transaction inclusion failed {_0}")]
    ExecutionError(Error),
    /// Relayed Transaction didn't specify high enough max gas
    #[display(
        fmt = "Insufficient max gas: Expected: {claimed_max_gas:?}, Actual: {actual_max_gas:?}"
    )]
    InsufficientMaxGas {
        /// The max gas claimed by the L1 transaction submitter
        claimed_max_gas: u64,
        /// The actual max gas used by the transaction
        actual_max_gas: u64,
    },
}

/// The status of a transaction after it is executed.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TransactionExecutionStatus {
    /// The id of the transaction.
    pub id: Bytes32,
    /// The result of the executed transaction.
    pub result: TransactionExecutionResult,
}

/// The result of transaction execution.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TransactionExecutionResult {
    /// Transaction was successfully executed.
    Success {
        /// The result of successful transaction execution.
        result: Option<ProgramState>,
        /// The receipts generated by the executed transaction.
        receipts: Vec<Receipt>,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
    },
    /// The execution of the transaction failed.
    Failed {
        /// The result of failed transaction execution.
        result: Option<ProgramState>,
        /// The receipts generated by the executed transaction.
        receipts: Vec<Receipt>,
        /// The total gas used by the transaction.
        total_gas: u64,
        /// The total fee paid by the transaction.
        total_fee: u64,
    },
}

impl TransactionExecutionResult {
    /// Get the receipts generated by the executed transaction.
    pub fn receipts(&self) -> &[Receipt] {
        match self {
            TransactionExecutionResult::Success { receipts, .. }
            | TransactionExecutionResult::Failed { receipts, .. } => receipts,
        }
    }

    /// Get the total gas used by the transaction.
    pub fn total_gas(&self) -> &u64 {
        match self {
            TransactionExecutionResult::Success { total_gas, .. }
            | TransactionExecutionResult::Failed { total_gas, .. } => total_gas,
        }
    }

    #[cfg(feature = "std")]
    /// Get the reason of the failed transaction execution.
    pub fn reason(receipts: &[Receipt], state: &Option<ProgramState>) -> String {
        receipts
            .iter()
            .find_map(|receipt| match receipt {
                // Format as `Revert($rA)`
                Receipt::Revert { ra, .. } => Some(format!("Revert({ra})")),
                // Display PanicReason e.g. `OutOfGas`
                Receipt::Panic { reason, .. } => Some(format!("{}", reason.reason())),
                _ => None,
            })
            .unwrap_or_else(|| format!("{:?}", &state))
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, derive_more::Display, derive_more::From)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Error {
    #[display(fmt = "Transaction id was already used: {_0:#x}")]
    TransactionIdCollision(Bytes32),
    #[display(fmt = "Too many transactions in the block")]
    TooManyTransactions,
    #[display(fmt = "output already exists")]
    OutputAlreadyExists,
    #[display(fmt = "The computed fee caused an integer overflow")]
    FeeOverflow,
    #[display(fmt = "The computed gas caused an integer overflow")]
    GasOverflow,
    #[display(fmt = "The block is missing `Mint` transaction.")]
    MintMissing,
    #[display(fmt = "Found the second entry of the `Mint` transaction in the block.")]
    MintFoundSecondEntry,
    #[display(fmt = "The `Mint` transaction has an unexpected index.")]
    MintHasUnexpectedIndex,
    #[display(fmt = "The last transaction in the block is not `Mint`.")]
    MintIsNotLastTransaction,
    #[display(fmt = "The `Mint` transaction mismatches expectations.")]
    MintMismatch,
    #[display(fmt = "Can't increase the balance of the coinbase contract: {_0}.")]
    CoinbaseCannotIncreaseBalance(String),
    #[display(fmt = "Coinbase amount mismatches with expected.")]
    CoinbaseAmountMismatch,
    #[display(fmt = "Coinbase gas price mismatches with expected.")]
    CoinbaseGasPriceMismatch,
    #[from]
    TransactionValidity(TransactionValidityError),
    // TODO: Replace with `fuel_core_storage::Error` when execution error will live in the
    //  `fuel-core-executor`.
    #[display(fmt = "got error during work with storage {_0}")]
    StorageError(String),
    #[display(fmt = "got error during work with relayer {_0}")]
    RelayerError(String),
    #[display(fmt = "occurred an error during block header generation {_0}")]
    BlockHeaderError(BlockHeaderError),
    #[display(fmt = "Transaction({transaction_id:#x}) execution error: {error:?}")]
    VmExecution {
        // TODO: Use `InterpreterError<StorageError>` when `InterpreterError` implements serde
        error: String,
        transaction_id: Bytes32,
    },
    #[display(fmt = "{_0:?}")]
    InvalidTransaction(CheckError),
    #[display(fmt = "Transaction doesn't match expected result: {transaction_id:#x}")]
    InvalidTransactionOutcome { transaction_id: Bytes32 },
    #[display(fmt = "The amount of charged fees is invalid")]
    InvalidFeeAmount,
    #[display(
        fmt = "Block generated during validation does not match the provided block"
    )]
    BlockMismatch,
    #[display(fmt = "No matching utxo for contract id ${_0:#x}")]
    ContractUtxoMissing(ContractId),
    #[display(fmt = "message already spent {_0:#x}")]
    MessageDoesNotExist(Nonce),
    #[display(fmt = "Expected input of type {_0}")]
    InputTypeMismatch(String),
    #[display(fmt = "Executing of the genesis block is not allowed")]
    ExecutingGenesisBlock,
    #[display(fmt = "The da height exceeded its maximum limit")]
    DaHeightExceededItsLimit,
    #[display(fmt = "Unable to find the previous block to fetch the DA height")]
    PreviousBlockIsNotFound,
    #[display(fmt = "The relayer gives incorrect messages for the requested da height")]
    RelayerGivesIncorrectMessages,
    #[display(fmt = "Consensus parameters not found for version {_0}")]
    ConsensusParametersNotFound(ConsensusParametersVersion),
    /// It is possible to occur untyped errors in the case of the upgrade.
    #[display(fmt = "Occurred untyped error: {_0}")]
    Other(String),
    /// Number of outputs is more than `u16::MAX`.
    #[display(fmt = "Number of outputs is more than `u16::MAX`")]
    TooManyOutputs,
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

impl From<CheckError> for Error {
    fn from(e: CheckError) -> Self {
        Self::InvalidTransaction(e)
    }
}

impl From<ValidityError> for Error {
    fn from(e: ValidityError) -> Self {
        Self::InvalidTransaction(CheckError::Validity(e))
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, derive_more::Display, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum TransactionValidityError {
    #[display(fmt = "Coin({_0:#x}) input was already spent")]
    CoinAlreadySpent(UtxoId),
    #[display(fmt = "The input coin({_0:#x}) doesn't match the coin from database")]
    CoinMismatch(UtxoId),
    #[display(fmt = "The specified coin({_0:#x}) doesn't exist")]
    CoinDoesNotExist(UtxoId),
    #[display(
        fmt = "Message({_0:#x}) is not yet spendable, as it's DA height is newer than this block allows"
    )]
    MessageSpendTooEarly(Nonce),
    #[display(
        fmt = "The specified message({_0:#x}) doesn't exist, possibly because it was already spent"
    )]
    MessageDoesNotExist(Nonce),
    #[display(fmt = "The input message({_0:#x}) doesn't match the relayer message")]
    MessageMismatch(Nonce),
    #[display(fmt = "The specified contract({_0:#x}) doesn't exist")]
    ContractDoesNotExist(ContractId),
    #[display(fmt = "Contract output index isn't valid: {_0:#x}")]
    InvalidContractInputIndex(UtxoId),
    #[display(fmt = "Transaction validity: {_0:#?}")]
    Validation(CheckError),
}

impl From<CheckError> for TransactionValidityError {
    fn from(e: CheckError) -> Self {
        Self::Validation(e)
    }
}

impl From<ValidityError> for TransactionValidityError {
    fn from(e: ValidityError) -> Self {
        Self::Validation(CheckError::Validity(e))
    }
}
