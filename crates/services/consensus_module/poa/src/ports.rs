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

#[async_trait::async_trait]
pub trait TransactionPool {
    /// Returns the number of pending transactions in the `TxPool`.
    async fn pending_number(&self) -> anyhow::Result<usize>;

    async fn total_consumable_gas(&self) -> anyhow::Result<u64>;

    async fn remove_txs(&self, tx_ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>>;

    async fn next_transaction_status_update(&mut self) -> TxStatus;
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
