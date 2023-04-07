use super::BlockImporterAdapter;
use crate::database::Database;
use fuel_core_p2p::ports::{
    BlockHeightImporter,
    P2pDb,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        primitives::BlockId,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

impl P2pDb for Database {
    fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        self.get_sealed_block_by_height(height)
    }

    fn get_sealed_header(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlockHeader>> {
        self.get_sealed_block_header_by_height(height)
    }

    fn get_transactions(
        &self,
        block_id: &BlockId,
    ) -> StorageResult<Option<Vec<Transaction>>> {
        self.get_transactions_on_block(block_id)
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
                .map(|result| result.sealed_block.entity.header().consensus.height),
        )
    }
}
