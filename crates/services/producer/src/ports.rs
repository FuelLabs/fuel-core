use async_trait::async_trait;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::DaBlockHeight,
    },
    fuel_tx::{
        Bytes32,
        Transaction,
    },
    fuel_types::BlockHeight,
    services::{
        block_producer::Components,
        executor::{
            Result as ExecutorResult,
            TransactionExecutionStatus,
            UncommittedResult,
        },
    },
};
use std::borrow::Cow;

pub trait BlockProducerDatabase: Send + Sync {
    /// Gets the committed block at the `height`.
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;
}

#[async_trait]
pub trait TxPool: Send + Sync {
    /// The source of the transactions used by the executor.
    type TxSource;

    /// Returns the source of includable transactions.
    fn get_source(
        &self,
        // could be used by the txpool to filter txs based on maturity
        block_height: BlockHeight,
    ) -> Self::TxSource;
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

pub trait Executor<TxSource>: Send + Sync {
    /// The database used by the executor.
    type Database;

    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn execute_without_commit(
        &self,
        component: Components<TxSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Self::Database>>>;
}

pub trait DryRunner: Send + Sync {
    /// Executes the block without committing it to the database. During execution collects the
    /// receipts to return them. The `utxo_validation` field can be used to disable the validation
    /// of utxos during execution.
    fn dry_run(
        &self,
        block: Components<Vec<Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>>;
}
