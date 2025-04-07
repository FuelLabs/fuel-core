use crate::{
    errors::CompressionError,
    storage,
    storage::CompressedBlocks,
};
use fuel_core_storage::{
    self,
    kv_store::KeyValueInspect,
    merkle::column::MerkleizedColumn,
    not_found,
    transactional::{
        Modifiable,
        StorageTransaction,
    },
    StorageAsMut,
    StorageSize,
};

/// Compressed block type alias
pub type CompressedBlock = fuel_core_compression::VersionedCompressedBlock;

/// Trait for getting the latest height of the compression storage
pub trait LatestHeight {
    /// Get the latest height of the compression storage
    fn latest_height(&self) -> Option<u32>;
}

/// Trait for interacting with storage that supports compression
pub trait CompressionStorage:
    KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>
    + Modifiable
    + Send
    + Sync
{
}

impl<T> CompressionStorage for T where
    T: KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>
        + Modifiable
        + Send
        + Sync
{
}

pub(crate) trait WriteCompressedBlock {
    fn write_compressed_block(
        &mut self,
        height: &u32,
        compressed_block: &CompressedBlock,
    ) -> crate::Result<usize>;
}

impl<Storage> WriteCompressedBlock for StorageTransaction<Storage>
where
    Storage:
        KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>,
{
    fn write_compressed_block(
        &mut self,
        height: &u32,
        compressed_block: &CompressedBlock,
    ) -> crate::Result<usize> {
        let height = (*height).into();
        self.storage_as_mut::<CompressedBlocks>()
            .insert(&height, compressed_block)
            .map_err(CompressionError::FailedToWriteCompressedBlock)?;

        // this should not hit the db, we get it from the transaction
        let size = StorageSize::<CompressedBlocks>::size_of_value(self, &height)
            .map_err(CompressionError::FailedToGetCompressedBlockSize)?;

        let Some(size) = size else {
            return Err(CompressionError::FailedToGetCompressedBlockSize(
                not_found!(CompressedBlocks),
            ));
        };

        Ok(size)
    }
}
