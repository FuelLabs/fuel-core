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
#[allow(missing_docs)]
pub enum Error {
    /// Error occurred during serialization or deserialization of the entity.
    #[display(fmt = "error performing serialization or deserialization")]
    Codec,
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
    /// Multiple heights found in the commit to the database.
    #[display(fmt = "Multiple heights found in the commit {heights:?}")]
    MultipleHeightsInCommit {
        /// List of heights found in the commit.
        heights: Vec<u64>,
    },
    /// Failed to advance the height.
    #[display(fmt = "Failed to advance the height")]
    FailedToAdvanceHeight,
    /// The new and old heights are not linked.
    #[display(
        fmt = "New and old heights are not linked: prev_height: {prev_height:#x}, new_height: {new_height:#x}"
    )]
    HeightsAreNotLinked {
        /// The old height.
        prev_height: u64,
        /// The new height.
        new_height: u64,
    },
    /// The new height is not found, but the old height is set.
    #[display(
        fmt = "The new height is not found, but the old height is set: prev_height: {prev_height:#x}"
    )]
    NewHeightIsNotSet {
        /// The old height known by the database.
        prev_height: u64,
    },
    #[display(fmt = "The historical database doesn't have any history yet")]
    NoHistoryIsAvailable,
    #[display(
        fmt = "The historical database doesn't have history for the requested height {requested_height:#x}, \
            the oldest available height is {oldest_available_height:#x}"
    )]
    NoHistoryForRequestedHeight {
        requested_height: u64,
        oldest_available_height: u64,
    },
    #[display(fmt = "Reached the end of the history")]
    ReachedEndOfHistory,

    /// Not related to database error.
    #[from]
    Other(anyhow::Error),
}

#[cfg(feature = "test-helpers")]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.to_string().eq(&other.to_string())
    }
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
        ExecutorError::StorageError(format!("{}", StorageError::from(e)))
    }
}

#[cfg(test)]
fuel_core_trace::enable_tracing!();
