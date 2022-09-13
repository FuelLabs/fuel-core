pub use crate::common::fuel_vm::storage::{
    ContractsAssets,
    ContractsInfo,
    ContractsRawCode,
    ContractsState,
};
use crate::{
    common::{
        fuel_storage::Mappable,
        fuel_tx::{
            Transaction,
            UtxoId,
        },
        fuel_types::{
            Address,
            Bytes32,
            MessageId,
        },
        fuel_vm::prelude::InterpreterError,
    },
    model::{
        Coin,
        ConsensusId,
        DaBlockHeight,
        Message,
        ValidatorId,
        ValidatorStake,
    },
    relayer::StakingDiff,
};
use std::io::ErrorKind;
use thiserror::Error;

/// The storage table of coins. Each [`Coin`](crate::model::Coin) is represented by unique `UtxoId`.
pub struct Coins;

impl Mappable for Coins {
    type Key = UtxoId;
    type SetValue = Coin;
    type GetValue = Self::SetValue;
}

/// The storage table of bridged Ethereum [`Message`](crate::model::Message)s.
pub struct Messages;

impl Mappable for Messages {
    type Key = MessageId;
    type SetValue = Message;
    type GetValue = Self::SetValue;
}

/// The storage table of confirmed transactions.
pub struct Transactions;

impl Mappable for Transactions {
    type Key = Bytes32;
    type SetValue = Transaction;
    type GetValue = Self::SetValue;
}

/// The storage table of delegate's indexes used by relayer.
/// Delegate index maps delegate `Address` with list of da block where delegation happened.
pub struct DelegatesIndexes;

impl Mappable for DelegatesIndexes {
    type Key = Address;
    type SetValue = [DaBlockHeight];
    type GetValue = Vec<DaBlockHeight>;
}

/// The storage table of relayer validators set.
pub struct ValidatorsSet;

impl Mappable for ValidatorsSet {
    type Key = ValidatorId;
    type SetValue = (ValidatorStake, Option<ConsensusId>);
    type GetValue = Self::SetValue;
}

/// The storage table of relayer staking diffs.
pub struct StakingDiffs;

impl Mappable for StakingDiffs {
    type Key = DaBlockHeight;
    type SetValue = StakingDiff;
    type GetValue = Self::SetValue;
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("error performing binary serialization")]
    Codec,
    #[error("Failed to initialize chain")]
    ChainAlreadyInitialized,
    #[error("Chain is not yet initialized")]
    ChainUninitialized,
    #[error("Invalid database version")]
    InvalidDatabaseVersion,
    #[error("error occurred in the underlying datastore `{0}`")]
    DatabaseError(Box<dyn std::error::Error + Send + Sync>),
}

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

#[derive(Debug, Error)]
pub enum KvStoreError {
    #[error("generic error occurred")]
    Error(Box<dyn std::error::Error + Send + Sync>),
    #[error("resource not found")]
    NotFound,
}

impl From<Error> for KvStoreError {
    fn from(e: Error) -> Self {
        KvStoreError::Error(Box::new(e))
    }
}

impl From<KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::DatabaseError(Box::new(e))
    }
}

impl From<KvStoreError> for std::io::Error {
    fn from(e: KvStoreError) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

impl From<Error> for InterpreterError {
    fn from(e: Error) -> Self {
        InterpreterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}

impl From<KvStoreError> for InterpreterError {
    fn from(e: KvStoreError) -> Self {
        InterpreterError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
