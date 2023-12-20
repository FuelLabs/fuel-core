//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(missing_docs)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]
#![deny(unused_variables)]

use fuel_core_storage::Error as StorageError;
use fuel_core_types::services::executor::Error as ExecutorError;

/// The error occurred during work with any of databases.
#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum Error {
    /// Error occurred during serialization or deserialization of the entity.
    #[display(fmt = "error performing serialization or deserialization")]
    Codec,
    /// Chain can be initialized once.
    #[display(fmt = "Failed to initialize chain")]
    ChainAlreadyInitialized,
    /// Chain should be initialized before usage.
    #[display(fmt = "Chain is not yet initialized")]
    ChainUninitialized,
    /// The version of database or data is invalid (possibly not migrated).
    #[display(
        fmt = "Invalid database version, expected {expected:#x}, found {found:#x}"
    )]
    InvalidDatabaseVersion {
        /// the current database version
        found: u32,
        /// the database version expected by this build of fuel-core
        expected: u32,
    },

    /// Not related to database error.
    #[from]
    Other(anyhow::Error),
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

impl From<Error> for StorageError {
    fn from(e: Error) -> Self {
        StorageError::DatabaseError(Box::new(e))
    }
}

impl From<Error> for ExecutorError {
    fn from(e: Error) -> Self {
        ExecutorError::StorageError(anyhow::anyhow!(StorageError::from(e)))
    }
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
