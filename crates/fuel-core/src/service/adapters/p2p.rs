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
        block::Block,
        consensus::Sealed,
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
};

#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        self.get_sealed_block_by_height(height)
    }

    async fn get_sealed_header(
        &self,
        height: BlockHeight,
    ) -> StorageResult<Option<SealedBlockHeader>> {
        let block_id = match self.get_block_id(height)? {
            Some(i) => i,
            None => return Ok(None),
        };
        self.get_sealed_block_header(&block_id)
    }

    async fn get_transactions(
        &self,
        block_id: BlockId,
    ) -> StorageResult<Option<Vec<Transaction>>> {
        Ok(self.get_sealed_block_by_id(&block_id)?.map(
            |Sealed {
                 entity: Block { transactions, .. },
                 ..
             }| transactions,
        ))
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
