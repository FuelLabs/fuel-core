use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    transactional::StorageTransaction,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        header::BlockHeader,
        primitives::DaBlockHeight,
    },
    fuel_asm::Word,
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    services::{
        block_importer::{
            BlockImportInfo,
            UncommittedResult as UncommittedImportResult,
        },
        executor::UncommittedResult as UncommittedExecutionResult,
        txpool::ArcPoolTx,
    },
    tai64::Tai64,
};

#[cfg_attr(test, mockall::automock)]
pub trait TransactionPool: Send + Sync {
    /// Returns the number of pending transactions in the `TxPool`.
    fn pending_number(&self) -> usize;

    fn total_consumable_gas(&self) -> u64;

    fn remove_txs(&self, tx_ids: Vec<TxId>) -> Vec<ArcPoolTx>;

    fn transaction_status_events(&self) -> BoxStream<TxId>;
}

#[cfg(test)]
use fuel_core_storage::test_helpers::EmptyStorage;

/// The source of transactions for the block.
pub enum TransactionsSource {
    /// The source of transactions for the block is the `TxPool`.
    TxPool,
    /// Use specific transactions for the block.
    SpecificTransactions(Vec<Transaction>),
}

#[cfg_attr(test, mockall::automock(type Database=EmptyStorage;))]
#[async_trait::async_trait]
pub trait BlockProducer: Send + Sync {
    type Database;

    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        source: TransactionsSource,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedExecutionResult<StorageTransaction<Self::Database>>>;
}

#[cfg_attr(test, mockall::automock(type Database=EmptyStorage;))]
#[async_trait::async_trait]
pub trait BlockImporter: Send + Sync {
    type Database;

    async fn commit_result(
        &self,
        result: UncommittedImportResult<StorageTransaction<Self::Database>>,
    ) -> anyhow::Result<()>;

    fn block_stream(&self) -> BoxStream<BlockImportInfo>;
}

#[cfg_attr(test, mockall::automock)]
/// The port for the database.
pub trait Database {
    /// Gets the block header at `height`.
    fn block_header(&self, height: &BlockHeight) -> StorageResult<BlockHeader>;

    /// Gets the block header BMT MMR root at `height`.
    fn block_header_merkle_root(&self, height: &BlockHeight) -> StorageResult<Bytes32>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the relayer.
pub trait RelayerPort {
    /// Wait for the relayer to be in sync with the given DA height
    /// if the `da_height` is within the range of the current
    /// relayer sync'd height - `max_da_lag`.
    async fn await_until_if_in_range(
        &self,
        da_height: &DaBlockHeight,
        max_da_lag: &DaBlockHeight,
    ) -> anyhow::Result<()>;
}

#[cfg_attr(test, mockall::automock)]
pub trait P2pPort: Send + Sync + 'static {
    /// Subscribe to reserved peers connection updates.
    fn reserved_peers_count(&self) -> BoxStream<usize>;
}

#[async_trait::async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait SyncPort: Send + Sync {
    /// await synchronization with the peers
    async fn sync_with_peers(&mut self) -> anyhow::Result<()>;
}
