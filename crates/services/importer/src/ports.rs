use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
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

pub trait Executor<Database>: Send + Sync {
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>>;
}

pub trait Database {
    /// Returns the latest block height.
    fn latest_block_height(&self) -> StorageResult<BlockHeight>;

    /// Assigns the `Consensus` data to the block under the `block_id`.
    /// Return the previous value at the `height`, if any.
    fn seal_block(
        &mut self,
        block_id: &BlockId,
        consensus: &Consensus,
    ) -> StorageResult<Option<Consensus>>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;

    /// Assigns the block header BMT MMR root to the block at the height `height`.
    /// Return the previous value at the `height`, if any.
    fn insert_block_header_merkle_root(
        &mut self,
        height: &BlockHeight,
        root: &Bytes32,
    ) -> StorageResult<Option<Bytes32>>;
}
