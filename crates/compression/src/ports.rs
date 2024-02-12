//! Ports this services requires to function.

use fuel_core_types::blockchain::block::Block;

#[async_trait::async_trait]
pub trait CompressPort {
    /// Compress the next block.
    async fn compress_next(&mut self, block: Block) -> anyhow::Result<Vec<u8>>;
}

#[async_trait::async_trait]
pub trait DecompressPort {
    /// Decompress the next block.
    async fn decompress_next(&mut self, block: Vec<u8>) -> anyhow::Result<Block>;
}
