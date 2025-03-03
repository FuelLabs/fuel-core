/// Compressed block type alias
pub type CompressedBlock = fuel_core_compression::VersionedCompressedBlock;

/// Trait for interacting with storage that supports compression
pub trait CompressionStorage {
    /// Write a compressed block to storage
    fn write_compressed_block(
        &mut self,
        compressed_block: &CompressedBlock,
    ) -> crate::Result<()>;
}
