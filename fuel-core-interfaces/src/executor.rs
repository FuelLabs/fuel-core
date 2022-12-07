use crate::{
    common::{
        fuel_tx::{
            CheckError,
            Receipt,
            TxId,
            UtxoId,
        },
        fuel_types::{
            Bytes32,
            ContractId,
            MessageId,
        },
        fuel_vm::backtrace::Backtrace,
    },
    db::KvStoreError,
    model::{
        BlockId,
        FuelBlock,
        PartialFuelBlock,
    },
    txpool::TransactionStatus,
};
use fuel_vm::fuel_tx::Transaction;
use std::error::Error as StdError;
use thiserror::Error;

/// Starting point for executing a block.
/// Production starts with a [`PartialFuelBlock`].
/// Validation starts with a full [`FuelBlock`].
pub type ExecutionBlock = ExecutionTypes<PartialFuelBlock, FuelBlock>;

/// Execution wrapper with only a single type.
pub type ExecutionType<T> = ExecutionTypes<T, T>;

#[derive(Debug, Clone, Copy)]
/// Execution wrapper where the types
/// depend on the type of execution.
pub enum ExecutionTypes<P, V> {
    /// Production mode where P is being produced.
    Production(P),
    /// Validation mode where V is being checked.
    Validation(V),
}

/// The result of transactions execution.
#[derive(Debug)]
pub struct ExecutionResult {
    /// Created block during the execution of transactions. It contains only valid transactions.
    pub block: FuelBlock,
    /// The list of skipped transactions with corresponding errors. Those transactions were
    /// not included in the block and didn't affect the state of the blockchain.
    pub skipped_transactions: Vec<(Transaction, Error)>,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
}

#[derive(Debug, Clone)]
pub struct TransactionExecutionStatus {
    pub id: Bytes32,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Copy)]
/// The kind of execution.
pub enum ExecutionKind {
    /// Producing a block.
    Production,
    /// Validating a block.
    Validation,
}

pub trait Executor: Sync + Send {
    fn execute(&self, block: ExecutionBlock) -> Result<ExecutionResult, Error>;

    fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> Result<Vec<Vec<Receipt>>, Error>;
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TransactionValidityError {
    #[error("Coin input was already spent")]
    CoinAlreadySpent(UtxoId),
    #[error("Coin has not yet reached maturity")]
    CoinHasNotMatured(UtxoId),
    #[error("The specified coin doesn't exist")]
    CoinDoesNotExist(UtxoId),
    #[error("The specified message was already spent")]
    MessageAlreadySpent(MessageId),
    #[error(
        "Message is not yet spendable, as it's DA height is newer than this block allows"
    )]
    MessageSpendTooEarly(MessageId),
    #[error("The specified message doesn't exist")]
    MessageDoesNotExist(MessageId),
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
    #[error("Datastore error occurred")]
    DataStoreError(Box<dyn StdError + Send + Sync>),
}

impl From<KvStoreError> for TransactionValidityError {
    fn from(e: KvStoreError) -> Self {
        Self::DataStoreError(Box::new(e))
    }
}

#[derive(Error, Debug)]
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
    #[error("corrupted block state")]
    CorruptedBlockState(Box<dyn StdError + Send + Sync>),
    #[error("Transaction({transaction_id:#x}) execution error: {error:?}")]
    VmExecution {
        error: fuel_vm::prelude::InterpreterError,
        transaction_id: Bytes32,
    },
    #[error(transparent)]
    InvalidTransaction(#[from] CheckError),
    #[error("Execution error with backtrace")]
    Backtrace(Box<Backtrace>),
    #[error("Transaction doesn't match expected result: {transaction_id:#x}")]
    InvalidTransactionOutcome { transaction_id: Bytes32 },
    #[error("Transaction root is invalid")]
    InvalidTransactionRoot,
    #[error("The amount of charged fees is invalid")]
    InvalidFeeAmount,
    #[error("Block id is invalid")]
    InvalidBlockId,
    #[error("No matching utxo for contract id ${0:#x}")]
    ContractUtxoMissing(ContractId),
}

impl ExecutionBlock {
    /// Get the hash of the full [`FuelBlock`] if validating.
    pub fn id(&self) -> Option<BlockId> {
        match self {
            ExecutionTypes::Production(_) => None,
            ExecutionTypes::Validation(v) => Some(v.id()),
        }
    }

    /// Get the transaction root from the full [`FuelBlock`] if validating.
    pub fn txs_root(&self) -> Option<Bytes32> {
        match self {
            ExecutionTypes::Production(_) => None,
            ExecutionTypes::Validation(v) => Some(v.header().transactions_root),
        }
    }
}

impl<P, V> ExecutionTypes<P, V> {
    /// Map the production type if producing.
    pub fn map_p<Q, F>(self, f: F) -> ExecutionTypes<Q, V>
    where
        F: FnOnce(P) -> Q,
    {
        match self {
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
            ExecutionTypes::Production(p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(v) => ExecutionTypes::Validation(f(v)),
        }
    }

    /// Get a reference version of the inner type.
    pub fn as_ref(&self) -> ExecutionTypes<&P, &V> {
        match *self {
            ExecutionTypes::Production(ref p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get a mutable reference version of the inner type.
    pub fn as_mut(&mut self) -> ExecutionTypes<&mut P, &mut V> {
        match *self {
            ExecutionTypes::Production(ref mut p) => ExecutionTypes::Production(p),
            ExecutionTypes::Validation(ref mut v) => ExecutionTypes::Validation(v),
        }
    }

    /// Get the kind of execution.
    pub fn to_kind(&self) -> ExecutionKind {
        match self {
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
            ExecutionTypes::Production(p) => f(p).map(ExecutionTypes::Production),
            ExecutionTypes::Validation(v) => f(v).map(ExecutionTypes::Validation),
        }
    }

    /// Get the inner type.
    pub fn into_inner(self) -> T {
        match self {
            ExecutionTypes::Production(t) | ExecutionTypes::Validation(t) => t,
        }
    }

    /// Split into the execution kind and the inner type.
    pub fn split(self) -> (ExecutionKind, T) {
        let kind = self.to_kind();
        (kind, self.into_inner())
    }
}

impl ExecutionKind {
    /// Wrap a type in this execution kind.
    pub fn wrap<T>(self, t: T) -> ExecutionType<T> {
        match self {
            ExecutionKind::Production => ExecutionTypes::Production(t),
            ExecutionKind::Validation => ExecutionTypes::Validation(t),
        }
    }
}

impl From<Backtrace> for Error {
    fn from(e: Backtrace) -> Self {
        Error::Backtrace(Box::new(e))
    }
}

impl From<KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}

impl From<crate::db::Error> for Error {
    fn from(e: crate::db::Error) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}

impl<T> core::ops::Deref for ExecutionType<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}

impl<T> core::ops::DerefMut for ExecutionType<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ExecutionTypes::Production(p) => p,
            ExecutionTypes::Validation(v) => v,
        }
    }
}
