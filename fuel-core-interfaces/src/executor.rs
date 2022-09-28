use crate::{
    common::{
        fuel_tx::{
            TxId,
            UtxoId,
            ValidationError,
        },
        fuel_types::{
            Bytes32,
            ContractId,
            MessageId,
        },
        fuel_vm::backtrace::Backtrace,
    },
    db::KvStoreError,
    model::FuelBlock,
};
use async_trait::async_trait;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionMode {
    Production,
    #[allow(dead_code)]
    Validation,
}

#[async_trait]
pub trait Executor: Sync + Send {
    async fn execute(
        &self,
        block: &mut FuelBlock,
        mode: ExecutionMode,
    ) -> Result<(), Error>;
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
    Validation(#[from] ValidationError),
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
    InvalidTransaction(#[from] ValidationError),
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
