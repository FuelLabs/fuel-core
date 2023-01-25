//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]

use fuel_core_storage::Error as StorageError;
use fuel_core_types::services::executor::Error as ExecutorError;
use std::{
    array::TryFromSliceError,
    io::ErrorKind,
};

/// The error occurred during work with any of databases.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error occurred during serialization or deserialization of the entity.
    #[error("error performing serialization or deserialization")]
    Codec,
    /// Chain can be initialized once.
    #[error("Failed to initialize chain")]
    ChainAlreadyInitialized,
    /// Chain should be initialized before usage.
    #[error("Chain is not yet initialized")]
    ChainUninitialized,
    /// The version of database or data is invalid (possibly not migrated).
    #[error("Invalid database version")]
    InvalidDatabaseVersion,
    /// Not related to database error.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self {
        std::io::Error::new(ErrorKind::Other, e)
    }
}

impl From<Error> for StorageError {
    fn from(e: Error) -> Self {
        StorageError::DatabaseError(Box::new(e))
    }
}

impl From<TryFromSliceError> for Error {
    fn from(e: TryFromSliceError) -> Self {
        Self::Other(anyhow::anyhow!(e))
    }
}

impl From<Error> for ExecutorError {
    fn from(e: Error) -> Self {
        ExecutorError::StorageError(Box::new(StorageError::from(e)))
    }
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
