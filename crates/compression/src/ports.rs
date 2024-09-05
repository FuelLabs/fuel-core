//! Ports this service requires to function.

use fuel_core_types::fuel_tx::{
    TxPointer,
    UtxoId,
};

#[async_trait::async_trait]
pub trait UtxoIdToPointer {
    async fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<(TxPointer, u16)>;
}

#[async_trait::async_trait]
pub trait TxPointerToUtxoId {
    async fn lookup(&self, tx_pointer: TxPointer, index: u16) -> anyhow::Result<UtxoId>;
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
