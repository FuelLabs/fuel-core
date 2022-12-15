//! Types related to transactions.
use fuel_vm_private::prelude::{
    CheckError,
    MessageId,
    TxId,
    UtxoId,
};
use std::error::Error as StdError;
use thiserror::Error;

#[allow(missing_docs)]
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
