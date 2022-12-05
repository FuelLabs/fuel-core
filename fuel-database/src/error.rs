use crate::InterpreterError;
use std::io::{
    Error as StdError,
    ErrorKind,
};

// TODO: Use either `KvStoreError` or `Error`

#[derive(thiserror::Error, Debug)]
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
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum KvStoreError {
    #[error("generic error occurred")]
    Error(Box<dyn std::error::Error + Send + Sync>),
    /// This error should be created with `not_found` macro.
    #[error("resource of type `{0}` was not found at the: {1}")]
    NotFound(&'static str, &'static str),
}

/// Creates `KvStoreError::NotFound` error with file and line information inside.
///
/// # Examples
///
/// ```
/// use fuel_database::not_found;
/// use fuel_database::tables::Messages;
///
/// let string_type = not_found!("BlockId");
/// let mappable_type = not_found!(Messages);
/// let mappable_path = not_found!(fuel_database::tables::Coins);
/// ```
#[macro_export]
macro_rules! not_found {
    ($name: literal) => {
        $crate::KvStoreError::NotFound($name, concat!(file!(), ":", line!()))
    };
    ($ty: path) => {
        $crate::KvStoreError::NotFound(
            ::core::any::type_name::<<$ty as $crate::tables::Mappable>::GetValue>(),
            concat!(file!(), ":", line!()),
        )
    };
}

impl From<Error> for StdError {
    fn from(e: Error) -> Self {
        StdError::new(ErrorKind::Other, e)
    }
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

impl From<KvStoreError> for StdError {
    fn from(e: KvStoreError) -> Self {
        StdError::new(ErrorKind::Other, e)
    }
}

impl From<Error> for InterpreterError {
    fn from(e: Error) -> Self {
        InterpreterError::Io(StdError::new(ErrorKind::Other, e))
    }
}

impl From<KvStoreError> for InterpreterError {
    fn from(e: KvStoreError) -> Self {
        InterpreterError::Io(StdError::new(ErrorKind::Other, e))
    }
}

impl From<KvStoreError> for fuel_core_interfaces::executor::Error {
    fn from(e: KvStoreError) -> Self {
        fuel_core_interfaces::executor::Error::CorruptedBlockState(Box::new(e))
    }
}

impl From<Error> for fuel_core_interfaces::executor::Error {
    fn from(e: Error) -> Self {
        fuel_core_interfaces::executor::Error::CorruptedBlockState(Box::new(e))
    }
}

impl From<KvStoreError> for fuel_core_interfaces::executor::TransactionValidityError {
    fn from(e: KvStoreError) -> Self {
        Self::DataStoreError(Box::new(e))
    }
}

#[cfg(test)]
mod test {
    use crate::{
        not_found,
        tables::Coins,
    };

    #[test]
    fn not_found_output() {
        #[rustfmt::skip]
        assert_eq!(
            format!("{}", not_found!("BlockId")),
            format!("resource of type `BlockId` was not found at the: {}:{}", file!(), line!() - 1)
        );
        #[rustfmt::skip]
        assert_eq!(
            format!("{}", not_found!(Coins)),
            format!("resource of type `fuel_core_interfaces::model::coin::Coin` was not found at the: {}:{}", file!(), line!() - 1)
        );
    }
}
