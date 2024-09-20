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
    fuel_types::Nonce,
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
}

/// Lookup for UTXO pointers used for compression.
pub trait UtxoIdToPointer {
    fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<CompressedUtxoId>;
}

/// Lookup for history of UTXOs and messages, used for decompression.
pub trait HistoryLookup {
    fn utxo_id(&self, c: CompressedUtxoId) -> anyhow::Result<UtxoId>;
    fn coin(&self, utxo_id: UtxoId) -> anyhow::Result<CoinInfo>;
    fn message(&self, nonce: Nonce) -> anyhow::Result<MessageInfo>;
}

/// Information about a coin.
#[derive(Debug, Clone)]
pub struct CoinInfo {
    pub owner: Address,
    pub amount: u64,
    pub asset_id: AssetId,
}

/// Information about a message.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub sender: Address,
    pub recipient: Address,
    pub amount: Word,
    pub data: Vec<u8>,
}
