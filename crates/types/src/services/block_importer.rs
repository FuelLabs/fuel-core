//! Types related to block importer service.

use crate::{
    blockchain::{
        header::BlockHeader,
        SealedBlock,
    },
    services::{
        executor::TransactionExecutionStatus,
        Uncommitted,
    },
};
use core::ops::Deref;
use std::sync::Arc;

/// The uncommitted result of the block importing.
pub type UncommittedResult<DatabaseTransaction> =
    Uncommitted<ImportResult, DatabaseTransaction>;

/// The alias for the `ImportResult` that can be shared between threads.
pub type SharedImportResult = Arc<dyn Deref<Target = ImportResult> + Send + Sync>;

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

impl Deref for ImportResult {
    type Target = Self;

    fn deref(&self) -> &Self::Target {
        self
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct BlockImportInfo {
    /// The header of the imported block.
    pub block_header: BlockHeader,
    /// The producer of the block.
    source: Source,
}

impl BlockImportInfo {
    /// Returns `true` if the block was created locally.
    pub fn is_locally_produced(&self) -> bool {
        self.source == Source::Local
    }

    /// Creates a new `BlockImportInfo` with source from the network.
    pub fn new_from_network(block_header: BlockHeader) -> Self {
        Self {
            block_header,
            source: Source::Network,
        }
    }
}

impl From<SharedImportResult> for BlockImportInfo {
    fn from(result: SharedImportResult) -> Self {
        Self {
            block_header: result.sealed_block.entity.header().clone(),
            source: result.source,
        }
    }
}

impl From<BlockHeader> for BlockImportInfo {
    fn from(block_header: BlockHeader) -> Self {
        Self {
            block_header,
            source: Default::default(),
        }
    }
}
