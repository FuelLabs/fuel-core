//! Ports this service requires to function.

use fuel_core_types::fuel_tx::{
    TxId,
    TxPointer,
};

#[async_trait::async_trait]
pub trait TxIdToPointer {
    async fn lookup(&self, tx_id: TxId) -> anyhow::Result<TxPointer>;
}

#[async_trait::async_trait]
pub trait TxPointerToId {
    async fn lookup(&self, tx_pointer: TxPointer) -> anyhow::Result<TxId>;
}

// Exposed interfaces: where should these live?

// use fuel_core_types::blockchain::block::Block;

// #[async_trait::async_trait]
// pub trait CompressPort {
//     /// Compress the next block.
//     async fn compress_next(&mut self, block: Block) -> anyhow::Result<Vec<u8>>;
// }

// #[async_trait::async_trait]
// pub trait DecompressPort {
//     /// Decompress the next block.
//     async fn decompress_next(&mut self, block: Vec<u8>) -> anyhow::Result<Block>;
// }
