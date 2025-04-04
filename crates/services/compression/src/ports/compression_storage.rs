use crate::storage;
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
        compressed_block: &crate::ports::compression_storage::CompressedBlock,
    ) -> crate::Result<usize> {
        self.storage_as_mut::<storage::CompressedBlocks>()
            .insert(&(*height).into(), compressed_block)
            .map_err(crate::errors::CompressionError::FailedToWriteCompressedBlock)?;

        // this should not hit the db, we get it from the transaction
        let size = KeyValueInspect::size_of_value(
            self,
            height.to_be_bytes().as_ref(),
            MerkleizedColumn::<storage::column::CompressionColumn>::TableColumn(
                storage::column::CompressionColumn::CompressedBlocks,
            ),
        )
        .map_err(crate::errors::CompressionError::FailedToGetCompressedBlockSize)?;

        let Some(size) = size else {
            return Err(
                crate::errors::CompressionError::FailedToGetCompressedBlockSize(
                    not_found!(storage::CompressedBlocks),
                ),
            );
        };

        Ok(size)
    }
}
