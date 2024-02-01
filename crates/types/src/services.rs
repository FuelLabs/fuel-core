//! Types for specific services

pub mod block_importer;
pub mod block_producer;
pub mod executor;
pub mod graphql_api;
pub mod p2p;
pub mod relayer;
pub mod txpool;

// TODO: Define a one common error for all services like

/// The uncommitted `Result` of some action with database transaction.
/// The user should commit the result by itself.
#[derive(Debug)]
pub struct Uncommitted<Result, DatabaseTransaction> {
    /// The result of the action.
    result: Result,
    /// The database transaction with not committed state.
    database_transaction: DatabaseTransaction,
}

impl<Result, DatabaseTransaction> Uncommitted<Result, DatabaseTransaction> {
    /// Create a new instance of `Uncommitted`.
    pub fn new(result: Result, database_transaction: DatabaseTransaction) -> Self {
        Self {
            result,
            database_transaction,
        }
    }

    /// Returns a reference to the `Result`.
    pub fn result(&self) -> &Result {
        &self.result
    }

    /// Return the result and database transaction.
    ///
    /// The caller can unpack the `Uncommitted`, apply some changes and pack it again into
    /// `UncommittedResult`. Because `commit` of the database transaction consumes `self`,
    /// after committing it is not possible create `Uncommitted`.
    pub fn into(self) -> (Result, DatabaseTransaction) {
        (self.result, self.database_transaction)
    }

    /// Discards the database transaction and returns only the result of the action.
    pub fn into_result(self) -> Result {
        self.result
    }

    /// Discards the result and return database transaction.
    pub fn into_transaction(self) -> DatabaseTransaction {
        self.database_transaction
    }
}
