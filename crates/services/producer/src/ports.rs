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
    fuel_tx::Receipt,
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
    /// Fetch previously committed block at given height.
    fn get_block(
        &self,
        fuel_height: BlockHeight,
    ) -> StorageResult<Option<Cow<CompressedBlock>>>;

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
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
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
