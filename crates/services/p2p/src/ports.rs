use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
};

pub trait P2pDb: Send + Sync {
    fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>>;

    fn get_sealed_header(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlockHeader>>;

    fn get_transactions(
        &self,
        block_id: &BlockId,
    ) -> StorageResult<Option<Vec<Transaction>>>;
}

pub trait BlockHeightImporter: Send + Sync {
    /// Creates a stream of next block heights
    fn next_block_height(&self) -> BoxStream<BlockHeight>;
}
