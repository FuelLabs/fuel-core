//! Ports this service requires to function.

use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        CompressedUtxoId,
        UtxoId,
        Word,
    },
    fuel_types::Nonce,
};

#[async_trait::async_trait]
pub trait UtxoIdToPointer {
    async fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<CompressedUtxoId>;
}

#[async_trait::async_trait]
pub trait HistoryLookup {
    async fn utxo_id(&self, c: &CompressedUtxoId) -> anyhow::Result<UtxoId>;
    async fn coin(&self, utxo_id: &UtxoId) -> anyhow::Result<CoinInfo>;
    async fn message(&self, nonce: &Nonce) -> anyhow::Result<MessageInfo>;
}

#[derive(Debug, Clone)]
pub struct CoinInfo {
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub sender: Address,
    pub recipient: Address,
    pub amount: Word,
    pub data: Vec<u8>,
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
