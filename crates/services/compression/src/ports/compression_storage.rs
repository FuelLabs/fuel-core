use crate::storage;
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    merkle::column::MerkleizedColumn,
    transactional::{
        Modifiable,
        StorageTransaction,
    },
    StorageAsMut,
};

/// Compressed block type alias
pub type CompressedBlock = fuel_core_compression::VersionedCompressedBlock;

/// Trait for interacting with storage that supports compression
pub trait CompressionStorage:
    KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>
    + Modifiable
{
}

impl<T> CompressionStorage for T where
    T: KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>
        + Modifiable
{
}

pub(crate) trait WriteCompressedBlock {
    fn write_compressed_block(
        &mut self,
        height: &u32,
        compressed_block: &crate::ports::compression_storage::CompressedBlock,
    ) -> crate::Result<()>;
}

impl<Storage> WriteCompressedBlock for StorageTransaction<Storage>
where
    Storage:
        KeyValueInspect<Column = MerkleizedColumn<storage::column::CompressionColumn>>,
{
    fn write_compressed_block(
        &mut self,
        height: &u32,
        compressed_block: &crate::ports::compression_storage::CompressedBlock,
    ) -> crate::Result<()> {
        self.storage_as_mut::<storage::CompressedBlocks>()
            .insert(&(*height).into(), compressed_block)
            .map_err(crate::errors::CompressionError::FailedToWriteCompressedBlock)
    }
}
