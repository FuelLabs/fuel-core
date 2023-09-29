use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::Transactions,
};
use std::ops::Range;

pub trait P2pDb: Send + Sync {
    fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>>;

    fn get_sealed_header(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlockHeader>>;

    fn get_sealed_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Vec<SealedBlockHeader>>;

    fn get_transactions(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>>;
}

pub trait BlockHeightImporter: Send + Sync {
    /// Creates a stream of next block heights
    fn next_block_height(&self) -> BoxStream<BlockHeight>;
}
