//! The crate `fuel-core-storage` contains storage types, primitives, tables used by `fuel-core`.
//! This crate doesn't contain the actual implementation of the storage. It works around the
//! `Database` and is used by services to provide a default implementation. Primitives
//! defined here are used by services but are flexible enough to customize the
//! logic when the `Database` is known.

#![deny(missing_docs)]

use fuel_core_types::blockchain::block::ExecutionResult;
pub use fuel_vm_private::fuel_storage::*;

pub mod tables;

/// The uncommitted result of transactions execution with database transaction.
/// The caller should commit the result by itself.
#[derive(Debug)]
pub struct UncommittedResult<DbTransaction> {
    /// The execution result.
    result: ExecutionResult,
    /// The database transaction with not committed state.
    database_transaction: DbTransaction,
}

impl<DbTransaction> UncommittedResult<DbTransaction> {
    /// Create a new instance of `UncommittedResult`.
    pub fn new(result: ExecutionResult, database_transaction: DbTransaction) -> Self {
        Self {
            result,
            database_transaction,
        }
    }

    /// Returns a reference to the `ExecutionResult`.
    pub fn result(&self) -> &ExecutionResult {
        &self.result
    }

    /// Return the result and database transaction.
    ///
    /// The service can unpack the `UncommittedResult`, apply some changes and pack it again into
    /// `UncommittedResult`. Because `commit` of the database transaction consumes `self`,
    /// after committing it is not possible create `UncommittedResult`.
    pub fn into(self) -> (ExecutionResult, DbTransaction) {
        (self.result, self.database_transaction)
    }

    /// Discards the database transaction and returns only the result of execution.
    pub fn into_result(self) -> ExecutionResult {
        self.result
    }

    /// Discards the result and return database transaction.
    pub fn into_transaction(self) -> DbTransaction {
        self.database_transaction
    }
}
