use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        consensus::Genesis,
        SealedBlockHeader,
    },
    fuel_tx::TxId,
    fuel_types::BlockHeight,
    services::p2p::{
        NetworkableTransactionPool,
        Transactions,
    },
};
use std::ops::Range;

pub trait P2pDb: Send + Sync {
    fn get_sealed_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>>;

    fn get_transactions(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>>;

    fn get_genesis(&self) -> StorageResult<Genesis>;
}

pub trait BlockHeightImporter: Send + Sync {
    /// Creates a stream of next block heights
    fn next_block_height(&self) -> BoxStream<BlockHeight>;
}

pub trait TxPool: Send + Sync + Clone {
    /// Get all tx ids in the pool
    fn get_tx_ids(&self, max_ids: usize) -> anyhow::Result<Vec<TxId>>;

    /// Get full txs from the pool
    fn get_full_txs(
        &self,
        tx_ids: Vec<TxId>,
    ) -> anyhow::Result<Vec<Option<NetworkableTransactionPool>>>;
}
