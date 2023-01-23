use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        primitives::{
            BlockHeight,
            BlockId,
        },
    },
    fuel_types::Bytes32,
    services::executor::{
        ExecutionBlock,
        Result as ExecutorResult,
        UncommittedResult,
    },
};

#[cfg_attr(test, mockall::automock(type Database = crate::importer::test::MockDatabase;))]
/// The executors port.
pub trait Executor: Send + Sync {
    /// The database used by the executor.
    type Database: ExecutorDatabase;

    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Self::Database>>>;
}

/// The database port used by the block importer.
pub trait ImporterDatabase {
    /// Returns the latest block height.
    fn latest_block_height(&self) -> StorageResult<BlockHeight>;
}

/// The port for returned database from the executor.
pub trait ExecutorDatabase: ImporterDatabase {
    /// Assigns the `Consensus` data to the block under the `block_id`.
    /// Return the previous value at the `height`, if any.
    fn seal_block(
        &mut self,
        block_id: &BlockId,
        consensus: &Consensus,
    ) -> StorageResult<Option<Consensus>>;

    /// Assigns the block header BMT MMR root to the block at the height `height`.
    /// Return the previous value at the `height`, if any.
    fn insert_block_header_merkle_root(
        &mut self,
        height: &BlockHeight,
        root: &Bytes32,
    ) -> StorageResult<Option<Bytes32>>;
}

#[cfg_attr(test, mockall::automock)]
/// The verifier of the block.
pub trait BlockVerifier {
    /// Verifies the consistency of the block fields for the block's height.
    /// It includes the verification of **all** fields, it includes the consensus rules for
    /// the corresponding height.
    ///
    /// Return an error if the verification failed, otherwise `Ok(())`.
    fn verify_block_fields(
        &self,
        consensus: &Consensus,
        block: &Block,
    ) -> anyhow::Result<()>;
}
