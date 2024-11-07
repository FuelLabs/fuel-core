use crate::graphql_api::ports::DatabaseDaCompressedBlocks;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_types::BlockHeight;

pub trait DaCompressedBlockData: Send + Sync {
    fn da_compressed_block(&self, id: &BlockHeight) -> StorageResult<Vec<u8>>;
}

impl<D> DaCompressedBlockData for D
where
    D: DatabaseDaCompressedBlocks + ?Sized + Send + Sync,
{
    fn da_compressed_block(&self, height: &BlockHeight) -> StorageResult<Vec<u8>> {
        self.da_compressed_block(height)
    }
}
