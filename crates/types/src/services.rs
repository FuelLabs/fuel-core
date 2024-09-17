//! Types for specific services

pub mod block_importer;
pub mod block_producer;
pub mod executor;
pub mod graphql_api;
#[cfg(feature = "std")]
pub mod p2p;
pub mod relayer;
#[cfg(feature = "std")]
pub mod txpool;

// TODO: Define a one common error for all services like

/// The uncommitted `Result` of some action with storage changes.
/// The user should commit the result by itself.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[must_use]
pub struct Uncommitted<Result, Changes> {
    /// The result of the action.
    result: Result,
    /// The storage changes.
    changes: Changes,
}

impl<Result, Changes> Uncommitted<Result, Changes> {
    /// Create a new instance of `Uncommitted`.
    pub fn new(result: Result, changes: Changes) -> Self {
        Self { result, changes }
    }

    /// Returns a reference to the `Result`.
    pub fn result(&self) -> &Result {
        &self.result
    }

    /// Return the result and storage changes.
    pub fn into(self) -> (Result, Changes) {
        (self.result, self.changes)
    }

    /// Discards the database transaction and returns only the result of the action.
    pub fn into_result(self) -> Result {
        self.result
    }

    /// Discards the result and return storage changes.
    pub fn into_changes(self) -> Changes {
        self.changes
    }
}
