use fuel_core_services::stream::BoxStream;
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    blockchain::{
        consensus::Consensus,
        primitives::{
            BlockHeight,
            BlockId,
        },
    },
    fuel_asm::Word,
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
    },
    services::{
        executor::UncommittedResult,
        txpool::{
            ArcPoolTx,
            TxStatus,
        },
    },
};

pub trait TransactionPool: Send + Sync {
    /// Returns the number of pending transactions in the `TxPool`.
    fn pending_number(&self) -> usize;

    fn total_consumable_gas(&self) -> u64;

    fn remove_txs(&self, tx_ids: Vec<TxId>) -> Vec<ArcPoolTx>;

    fn transaction_status_events(&self) -> BoxStream<TxStatus>;
}

pub trait BlockDb: Send + Sync {
    fn block_height(&self) -> anyhow::Result<BlockHeight>;

    // Returns error if already sealed
    fn seal_block(
        &mut self,
        block_id: BlockId,
        consensus: Consensus,
    ) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait BlockProducer<Database>: Send + Sync {
    // TODO: Right now production and execution of the block is one step, but in the future,
    //  `produce_block` should only produce a block without affecting the blockchain state.
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<Database>>>;

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}
