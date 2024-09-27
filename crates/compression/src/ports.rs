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

/// Rolling cache for compression.
/// Holds the latest state which can be event sourced from the compressed blocks.
/// The changes done using this trait in a single call to `compress` or `decompress`
/// must be committed atomically, after which block height must be incremented.
pub trait TemporalRegistry<T> {
    /// Reads a value from the registry at its current height.
    fn read_registry(&self, key: &RegistryKey) -> anyhow::Result<T>;

    /// Reads a value from the registry at its current height.
    fn write_registry(&mut self, key: &RegistryKey, value: &T) -> anyhow::Result<()>;

    /// Lookup registry key by the value.
    fn registry_index_lookup(&self, value: &T) -> anyhow::Result<Option<RegistryKey>>;
}

impl<D, T> TemporalRegistry<T> for &mut D
where
    D: TemporalRegistry<T>,
{
    fn read_registry(&self, key: &RegistryKey) -> anyhow::Result<T> {
        <D as TemporalRegistry<T>>::read_registry(self, key)
    }

    fn write_registry(&mut self, key: &RegistryKey, value: &T) -> anyhow::Result<()> {
        <D as TemporalRegistry<T>>::write_registry(self, key, value)
    }

    fn registry_index_lookup(&self, value: &T) -> anyhow::Result<Option<RegistryKey>> {
        <D as TemporalRegistry<T>>::registry_index_lookup(self, value)
    }
}

/// Lookup for UTXO pointers used for compression.
pub trait UtxoIdToPointer {
    fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<CompressedUtxoId>;
}

impl<D> UtxoIdToPointer for &mut D
where
    D: UtxoIdToPointer,
{
    fn lookup(&self, utxo_id: UtxoId) -> anyhow::Result<CompressedUtxoId> {
        <D as UtxoIdToPointer>::lookup(self, utxo_id)
    }
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

/// Evictor registry to keep track of the latest used key for the type `T`.
pub trait EvictorDb<T> {
    fn get_latest_assigned_key(&self) -> anyhow::Result<RegistryKey>;
    fn set_latest_assigned_key(&mut self, key: RegistryKey) -> anyhow::Result<()>;
}

impl<D, T> EvictorDb<T> for &mut D
where
    D: EvictorDb<T>,
{
    fn get_latest_assigned_key(&self) -> anyhow::Result<RegistryKey> {
        <D as EvictorDb<T>>::get_latest_assigned_key(self)
    }

    fn set_latest_assigned_key(&mut self, key: RegistryKey) -> anyhow::Result<()> {
        <D as EvictorDb<T>>::set_latest_assigned_key(self, key)
    }
}
