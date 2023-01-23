use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    fuel_asm::Word,
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
    },
    services::{
        block_importer::UncommittedResult as UncommittedImportResult,
        executor::UncommittedResult as UncommittedExecutionResult,
        txpool::{
            ArcPoolTx,
            TxStatus,
        },
    },
    tai64::Tai64,
};

#[cfg_attr(test, mockall::automock)]
pub trait TransactionPool: Send + Sync {
    /// Returns the number of pending transactions in the `TxPool`.
    fn pending_number(&self) -> usize;

    fn total_consumable_gas(&self) -> u64;

    fn remove_txs(&self, tx_ids: Vec<TxId>) -> Vec<ArcPoolTx>;

    fn transaction_status_events(&self) -> BoxStream<TxStatus>;
}

#[cfg(test)]
use fuel_core_storage::test_helpers::EmptyStorage;

#[cfg_attr(test, mockall::automock(type Database=EmptyStorage;))]
#[async_trait::async_trait]
pub trait BlockProducer: Send + Sync {
    type Database;

    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Option<Tai64>,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedExecutionResult<StorageTransaction<Self::Database>>>;

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}

#[cfg_attr(test, mockall::automock(type Database=EmptyStorage;))]
pub trait BlockImporter: Send + Sync {
    type Database;

    fn commit_result(
        &self,
        result: UncommittedImportResult<StorageTransaction<Self::Database>>,
    ) -> anyhow::Result<()>;
}
