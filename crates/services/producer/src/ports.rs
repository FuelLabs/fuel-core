use async_trait::async_trait;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
    },
    fuel_tx::{
        Bytes32,
        Receipt,
    },
    services::{
        executor::{
            ExecutionBlock,
            Result as ExecutorResult,
            UncommittedResult,
        },
        txpool::ArcPoolTx,
    },
};
use std::borrow::Cow;

pub trait BlockProducerDatabase: Send + Sync {
    /// Gets the committed block at the `height`.
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;

    /// Fetch the current block height.
    fn current_block_height(&self) -> StorageResult<BlockHeight>;
}

#[async_trait]
pub trait TxPool: Send + Sync {
    fn get_includable_txs(
        &self,
        // could be used by the txpool to filter txs based on maturity
        block_height: BlockHeight,
        // The upper limit for the total amount of gas of these txs
        max_gas: u64,
    ) -> Vec<ArcPoolTx>;
}

#[async_trait::async_trait]
pub trait Relayer: Send + Sync {
    /// Wait for the relayer to reach at least this height and return the
    /// latest height (which is guaranteed to be >= height).
    async fn wait_for_at_least(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight>;
}

pub trait Executor<Database>: Send + Sync {
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>>;

    /// Executes the block without committing it to the database. During execution collects the
    /// receipts to return them. The `utxo_validation` field can be used to disable the validation
    /// of utxos during execution.
    fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>>;
}
