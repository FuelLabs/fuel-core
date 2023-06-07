//! Types related to block importer service.

use fuel_vm_private::fuel_types::BlockHeight;

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
#[cfg_attr(any(test, feature = "test-helpers"), derive(Default))]
pub struct ImportResult {
    /// Imported sealed block.
    pub sealed_block: SealedBlock,
    /// The status of the transactions execution included into the block.
    pub tx_status: Vec<TransactionExecutionStatus>,
    /// The source producer of the block.
    pub source: Source,
}

/// The source producer of the block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Source {
    /// The block was imported from the network.
    Network,
    /// The block was produced locally.
    #[default]
    Local,
}

impl ImportResult {
    /// Creates a new `ImportResult` from the local producer.
    pub fn new_from_local(
        sealed_block: SealedBlock,
        tx_status: Vec<TransactionExecutionStatus>,
    ) -> Self {
        Self {
            sealed_block,
            tx_status,
            source: Source::Local,
        }
    }

    /// Creates a new `ImportResult` from the network.
    pub fn new_from_network(
        sealed_block: SealedBlock,
        tx_status: Vec<TransactionExecutionStatus>,
    ) -> Self {
        Self {
            sealed_block,
            tx_status,
            source: Source::Network,
        }
    }
}

/// The block import info.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockImportInfo {
    /// The height of the imported block.
    pub height: BlockHeight,
    /// The producer of the block.
    source: Source,
}

impl BlockImportInfo {
    /// Returns `true` if the block was created locally.
    pub fn is_locally_produced(&self) -> bool {
        self.source == Source::Local
    }

    /// Creates a new `BlockImportInfo` with source from the network.
    pub fn new_from_network(height: BlockHeight) -> Self {
        Self {
            height,
            source: Source::Network,
        }
    }
}

impl From<&ImportResult> for BlockImportInfo {
    fn from(result: &ImportResult) -> Self {
        Self {
            height: *result.sealed_block.entity.header().height(),
            source: result.source,
        }
    }
}

impl From<BlockHeight> for BlockImportInfo {
    fn from(height: BlockHeight) -> Self {
        Self {
            height,
            source: Default::default(),
        }
    }
}
