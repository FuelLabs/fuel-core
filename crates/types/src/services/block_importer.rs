//! Types related to block importer service.

use crate::{
    blockchain::SealedBlock,
    services::{
        executor::TransactionExecutionStatus,
        Uncommitted,
    },
};

/// The uncommitted result of the block importing.
pub type UncommittedResult<DatabaseTransaction> =
    Uncommitted<ImportResult, DatabaseTransaction>;

/// The result of the block import.
#[derive(Debug)]
pub struct ImportResult {
    /// Imported sealed block.
    pub sealed_block: SealedBlock,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
}
