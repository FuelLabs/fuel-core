use async_trait::async_trait;
use fuel_core_storage::{
    transactional::Changes,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
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
    /// Returns the latest block height.
    fn latest_height(&self) -> Option<BlockHeight>;

    /// Gets the committed block at the `height`.
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;

    /// Returns the latest consensus parameters version.
    fn latest_consensus_parameters_version(
        &self,
    ) -> StorageResult<ConsensusParametersVersion>;

    /// Returns the latest state transition bytecode version.
    fn latest_state_transition_bytecode_version(
        &self,
    ) -> StorageResult<StateTransitionBytecodeVersion>;
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
    /// Wait for the relayer to reach at least this height and return the latest height.
    async fn wait_for_at_least_height(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight>;

    /// Get the total Forced Transaction gas cost for the block at the given height.
    async fn get_cost_for_block(&self, height: &DaBlockHeight) -> anyhow::Result<u64>;
}

pub trait BlockProducer<TxSource>: Send + Sync {
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn produce_without_commit(
        &self,
        component: Components<TxSource>,
    ) -> ExecutorResult<UncommittedResult<Changes>>;
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
