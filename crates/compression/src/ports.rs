//! Ports this service requires to function.

use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        Address,
        AssetId,
        CompressedUtxoId,
        UtxoId,
        Word,
    },
    fuel_types::{
        BlockHeight,
        Nonce,
    },
};

use crate::tables::RegistryKeyspace;

/// Rolling cache for compression.
/// Holds the latest state which can be event sourced from the compressed blocks.
/// The changes done using this trait in a single call to `compress` or `decompress`
/// must be committed atomically, after which block height must be incremented.
pub trait TemporalRegistry {
    /// Reads a value from the registry at its current height.
    fn read_registry(
        &self,
        keyspace: RegistryKeyspace,
        key: RegistryKey,
    ) -> anyhow::Result<Vec<u8>>;

    /// Reads a value from the registry at its current height.
    fn write_registry(
        &mut self,
        keyspace: RegistryKeyspace,
        key: RegistryKey,
        value: Vec<u8>,
    ) -> anyhow::Result<()>;

    /// Lookup registry key by the value.
    fn registry_index_lookup(
        &self,
        keyspace: RegistryKeyspace,
        value: Vec<u8>,
    ) -> anyhow::Result<Option<RegistryKey>>;

    /// Get the block height for the next block, i.e. the block currently being processed.
    fn next_block_height(&self) -> anyhow::Result<BlockHeight>;
}

pub trait UtxoIdToPointer {
    fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<CompressedUtxoId>;
}

pub trait HistoryLookup {
    fn utxo_id(&self, c: &CompressedUtxoId) -> anyhow::Result<UtxoId>;
    fn coin(&self, utxo_id: &UtxoId) -> anyhow::Result<CoinInfo>;
    fn message(&self, nonce: &Nonce) -> anyhow::Result<MessageInfo>;
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
