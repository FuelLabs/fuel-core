use super::BlockImporterAdapter;
use crate::database::OnChainIterableKeyValueView;
use fuel_core_p2p::ports::{
    BlockHeightImporter,
    P2pDb,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        consensus::Genesis,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::Transactions,
};
use std::ops::Range;

impl P2pDb for OnChainIterableKeyValueView {
    fn get_sealed_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<SealedBlockHeader>>> {
        self.get_sealed_block_headers(block_height_range)
    }

    fn get_transactions(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>> {
        self.get_transactions_on_blocks(block_height_range)
    }

    fn get_genesis(&self) -> StorageResult<Genesis> {
        self.get_genesis()
    }
}

impl BlockHeightImporter for BlockImporterAdapter {
    fn next_block_height(&self) -> BoxStream<BlockHeight> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|result| result.ok())
                .map(|result| *result.sealed_block.entity.header().height()),
        )
    }
}
