//! Types related to executor service.

use crate::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        primitives::BlockId,
    },
    fuel_tx::{
        CheckError,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::{
        Bytes32,
        ContractId,
        Nonce,
    },
    fuel_vm::{
        Backtrace,
        InterpreterError,
        ProgramState,
    },
    services::Uncommitted,
};
use std::error::Error as StdError;

/// The alias for executor result.
pub type Result<T> = core::result::Result<T, Error>;
/// The uncommitted result of the transaction execution.
pub type UncommittedResult<DatabaseTransaction> =
    Uncommitted<ExecutionResult, DatabaseTransaction>;

/// The result of transactions execution.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Created block during the execution of transactions. It contains only valid transactions.
    pub block: Block,
    /// The list of skipped transactions with corresponding errors. Those transactions were
    /// not included in the block and didn't affect the state of the blockchain.
    pub skipped_transactions: Vec<(Transaction, Error)>,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
}

/// The status of a transaction after it is executed.
#[derive(Debug, Clone)]
pub struct TransactionExecutionStatus {
    /// The id of the transaction.
    pub id: Bytes32,
    /// The result of the executed transaction.
    pub result: TransactionExecutionResult,
}

/// The result of transaction execution.
#[derive(Debug, Clone)]
pub enum TransactionExecutionResult {
    /// Transaction was successfully executed.
    Success {
        /// The result of successful transaction execution.
        result: Option<ProgramState>,
    },
    /// The execution of the transaction failed.
    Failed {
        /// The result of failed transaction execution.
        result: Option<ProgramState>,
        /// The reason of execution failure.
        reason: String,
    },
}

/// Execution wrapper where the types
/// depend on the type of execution.
#[derive(Debug, Clone, Copy)]
pub enum ExecutionTypes<P, V> {
    /// DryRun mode where P is being produced.
    DryRun(P),
    /// Production mode where P is being produced.
    Production(P),
    /// Validation mode where V is being checked.
    Validation(V),
}

/// Starting point for executing a block. Production starts with a [`PartialFuelBlock`].
/// Validation starts with a full [`FuelBlock`].
pub type ExecutionBlock = ExecutionTypes<PartialFuelBlock, Block>;

impl ExecutionBlock {
    /// Get the hash of the full [`FuelBlock`] if validating.
    pub fn id(&self) -> Option<BlockId> {
        match self {
            ExecutionTypes::DryRun(_) => None,
            ExecutionTypes::Production(_) => None,
            ExecutionTypes::Validation(v) => Some(v.id()),
        }
    }
}

// TODO: Move `ExecutionType` and `ExecutionKind` into `fuel-core-executor`

/// Execution wrapper with only a single type.
pub type ExecutionType<T> = ExecutionTypes<T, T>;

impl<P, V> ExecutionTypes<P, V> {
    /// Map the production type if producing.
    pub fn map_p<Q, F>(self, f: F) -> ExecutionTypes<Q, V>
    where
        F: FnOnce(P) -> Q,
    {
        match self {
            ExecutionTypes::DryRun(p) => ExecutionTypes::DryRun(f(p)),
            ExecutionTypes::Production(p) => ExecutionTypes::Production(f(p)),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(v),
        }
    }

    /// Map the validation type if validating.
    pub fn map_v<W, F>(self, f: F) -> ExecutionTypes<P, W>
    where
        F: FnOnce(V) -> W,
    {
        match self {
            ExecutionTypes::DryRun(p) => ExecutionTypes::DryRun(p),
            ExecutionTypes::Production(p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(f(v)),
        }
    }

    /// Get a reference version of the inner type.
    pub fn as_ref(&self) -> ExecutionTypes<&P, &V> {
        match *self {
            ExecutionTypes::DryRun(ref p) => ExecutionTypes::DryRun(p),
            ExecutionTypes::Production(ref p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get a mutable reference version of the inner type.
    pub fn as_mut(&mut self) -> ExecutionTypes<&mut P, &mut V> {
        match *self {
            ExecutionTypes::DryRun(ref mut p) => ExecutionTypes::DryRun(p),
            ExecutionTypes::Production(ref mut p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref mut v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get the kind of execution.
    pub fn to_kind(&self) -> ExecutionKind {
        match self {
            ExecutionTypes::DryRun(_) => ExecutionKind::DryRun,
            ExecutionTypes::Production(_) => ExecutionKind::Production,
            ExecutionTypes::Validation(_) => ExecutionKind::Validation,
        }
    }
}

impl<T> ExecutionType<T> {
    /// Map the wrapped type.
    pub fn map<U, F>(self, f: F) -> ExecutionType<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            ExecutionTypes::DryRun(p) => ExecutionTypes::DryRun(f(p)),
            ExecutionTypes::Production(p) => ExecutionTypes::Production(f(p)),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(f(v)),
        }
    }

    /// Filter and map the inner type.
    pub fn filter_map<U, F>(self, f: F) -> Option<ExecutionType<U>>
    where
        F: FnOnce(T) -> Option<U>,
    {
        match self {
            ExecutionTypes::DryRun(p) => f(p).map(ExecutionTypes::DryRun),
            ExecutionTypes::Production(p) => f(p).map(ExecutionTypes::Production),
            ExecutionTypes::Validation(v) => f(v).map(ExecutionTypes::Validation),
        }
    }

    /// Get the inner type.
    pub fn into_inner(self) -> T {
        match self {
            ExecutionTypes::DryRun(t)
            | ExecutionTypes::Production(t)
            | ExecutionTypes::Validation(t) => t,
        }
    }

    /// Split into the execution kind and the inner type.
    pub fn split(self) -> (ExecutionKind, T) {
        let kind = self.to_kind();
        (kind, self.into_inner())
    }
}

impl<T> core::ops::Deref for ExecutionType<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ExecutionTypes::DryRun(p) => p,
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}

impl<T> core::ops::DerefMut for ExecutionType<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ExecutionTypes::DryRun(p) => p,
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The kind of execution.
pub enum ExecutionKind {
    /// Dry run a block.
    DryRun,
    /// Producing a block.
    Production,
    /// Validating a block.
    Validation,
}

impl ExecutionKind {
    /// Wrap a type in this execution kind.
    pub fn wrap<T>(self, t: T) -> ExecutionType<T> {
        match self {
            ExecutionKind::DryRun => ExecutionTypes::DryRun(t),
            ExecutionKind::Production => ExecutionTypes::Production(t),
            ExecutionKind::Validation => ExecutionTypes::Validation(t),
        }
    }
}

#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Transaction id was already used: {0:#x}")]
    TransactionIdCollision(Bytes32),
    #[error("output already exists")]
    OutputAlreadyExists,
    #[error("The computed fee caused an integer overflow")]
    FeeOverflow,
    #[error("Not supported transaction: {0:?}")]
    NotSupportedTransaction(Box<Transaction>),
    #[error("The first transaction in the block is not `Mint` - coinbase.")]
    CoinbaseIsNotFirstTransaction,
    #[error("Coinbase should have one output.")]
    CoinbaseSeveralOutputs,
    #[error("Coinbase outputs is invalid.")]
    CoinbaseOutputIsInvalid,
    #[error("Coinbase amount mismatches with expected.")]
    CoinbaseAmountMismatch,
    #[error("Invalid transaction: {0}")]
    TransactionValidity(#[from] TransactionValidityError),
    // TODO: Replace with `fuel_core_storage::Error` when execution error will live in the
    //  `fuel-core-executor`.
    #[error("got error during work with storage {0}")]
    StorageError(Box<dyn StdError + Send + Sync>),
    #[error("got error during work with relayer {0}")]
    RelayerError(Box<dyn StdError + Send + Sync>),
    #[error("Transaction({transaction_id:#x}) execution error: {error:?}")]
    VmExecution {
        error: InterpreterError,
        transaction_id: Bytes32,
    },
    #[error(transparent)]
    InvalidTransaction(#[from] CheckError),
    #[error("Execution error with backtrace")]
    Backtrace(Box<Backtrace>),
    #[error("Transaction doesn't match expected result: {transaction_id:#x}")]
    InvalidTransactionOutcome { transaction_id: Bytes32 },
    #[error("The amount of charged fees is invalid")]
    InvalidFeeAmount,
    #[error("Block id is invalid")]
    InvalidBlockId,
    #[error("No matching utxo for contract id ${0:#x}")]
    ContractUtxoMissing(ContractId),
    #[error("message already spent {0:#x}")]
    MessageAlreadySpent(Nonce),
    #[error("Expected input of type {0}")]
    InputTypeMismatch(String),
}

impl From<Backtrace> for Error {
    fn from(e: Backtrace) -> Self {
        Error::Backtrace(Box::new(e))
    }
}

#[allow(missing_docs)]
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TransactionValidityError {
    #[error("Coin input was already spent")]
    CoinAlreadySpent(UtxoId),
    #[error("Coin has not yet reached maturity")]
    CoinHasNotMatured(UtxoId),
    #[error("The specified coin doesn't exist")]
    CoinDoesNotExist(UtxoId),
    #[error("The specified message was already spent")]
    MessageAlreadySpent(Nonce),
    #[error(
        "Message is not yet spendable, as it's DA height is newer than this block allows"
    )]
    MessageSpendTooEarly(Nonce),
    #[error("The specified message doesn't exist")]
    MessageDoesNotExist(Nonce),
    #[error("The input message sender doesn't match the relayer message sender")]
    MessageSenderMismatch(Nonce),
    #[error("The input message recipient doesn't match the relayer message recipient")]
    MessageRecipientMismatch(Nonce),
    #[error("The input message amount doesn't match the relayer message amount")]
    MessageAmountMismatch(Nonce),
    #[error("The input message nonce doesn't match the relayer message nonce")]
    MessageNonceMismatch(Nonce),
    #[error("The input message data doesn't match the relayer message data")]
    MessageDataMismatch(Nonce),
    #[error("Contract output index isn't valid: {0:#x}")]
    InvalidContractInputIndex(UtxoId),
    #[error("The transaction must have at least one coin or message input type: {0:#x}")]
    NoCoinOrMessageInput(TxId),
    #[error("The transaction contains predicate inputs which aren't enabled: {0:#x}")]
    PredicateExecutionDisabled(TxId),
    #[error(
    "The transaction contains a predicate which failed to validate: TransactionId({0:#x})"
    )]
    InvalidPredicate(TxId),
    #[error("Transaction validity: {0:#?}")]
    Validation(#[from] CheckError),
}
