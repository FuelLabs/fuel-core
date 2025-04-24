use fuel_core_storage::{
    Result as StorageResult,
    transactional::Changes,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
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
            DryRunResult,
            Result as ExecutorResult,
            StorageReadReplayEvent,
            UncommittedResult,
        },
    },
};
use std::{
    borrow::Cow,
    future::Future,
};

pub trait BlockProducerDatabase: Send + Sync {
    /// Returns the latest block height.
    fn latest_height(&self) -> Option<BlockHeight>;

    /// Gets the committed block at the `height`.
    fn get_block(&self, height: &BlockHeight) -> StorageResult<Cow<CompressedBlock>>;

    /// Gets the full committed block at the `height`.
    fn get_full_block(&self, height: &BlockHeight) -> StorageResult<Block>;

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

pub trait TxPool: Send + Sync {
    /// The source of the transactions used by the executor.
    type TxSource;

    /// Returns the source of includable transactions.
    #[allow(async_fn_in_trait)]
    async fn get_source(
        &self,
        gas_price: u64,
        // could be used by the txpool to filter txs based on maturity
        block_height: BlockHeight,
    ) -> anyhow::Result<Self::TxSource>;
}

pub struct RelayerBlockInfo {
    pub gas_cost: u64,
    pub tx_count: u64,
}

#[async_trait::async_trait]
pub trait Relayer: Send + Sync {
    /// Wait for the relayer to reach at least this height and return the latest height.
    async fn wait_for_at_least_height(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<DaBlockHeight>;

    /// Get the total Forced Transaction gas cost for the block at the given height.
    async fn get_cost_and_transactions_number_for_block(
        &self,
        height: &DaBlockHeight,
    ) -> anyhow::Result<RelayerBlockInfo>;
}

pub trait BlockProducer<TxSource>: Send + Sync {
    type Deadline;
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn produce_without_commit(
        &self,
        component: Components<TxSource>,
        deadline: Self::Deadline,
    ) -> impl Future<Output = ExecutorResult<UncommittedResult<Changes>>>;
}

pub trait DryRunner: Send + Sync {
    /// Executes the block without committing it to the database. During execution collects the
    /// receipts to return them. The `forbid_fake_coins` field can be used to enable/disable the validation
    /// of utxos during execution. The `at_height` field can be used to dry run on top of a past block.
    fn dry_run(
        &self,
        block: Components<Vec<Transaction>>,
        forbid_fake_coins: Option<bool>,
        at_height: Option<BlockHeight>,
        record_storage_read_replay: bool,
    ) -> ExecutorResult<DryRunResult>;
}

pub trait StorageReadReplayRecorder: Send + Sync {
    fn storage_read_replay(
        &self,
        block: &Block,
    ) -> ExecutorResult<Vec<StorageReadReplayEvent>>;
}
