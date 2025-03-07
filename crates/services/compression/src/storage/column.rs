//! Column enum for the database.

use fuel_core_storage::{
    kv_store::StorageColumn,
    merkle::{
        column::{
            AsU32,
            MerkleizedColumn,
        },
        sparse::MerkleizedTableColumn,
    },
};

/// Enum representing the columns in the storage.
#[repr(u32)]
#[derive(
    Copy,
    Clone,
    Debug,
    strum_macros::EnumCount,
    strum_macros::IntoStaticStr,
    PartialEq,
    Eq,
    enum_iterator::Sequence,
    Hash,
)]
pub enum CompressionColumn {
    /// CompressedBlocks, see [`CompressedBlocks`](crate::storage::compressed_blocks::CompressedBlocks)
    CompressedBlocks = 0,
    /// RegistryKey to Address index, see [`Address`](crate::storage::address::Address)
    Address = 1,
    /// RegistryKey to AssetId index, see [`AssetId`](crate::storage::asset_id::AssetId)
    AssetId = 2,
    /// RegistryKey to ContractId index, see [`ContractId`](crate::storage::contract_id::ContractId)
    ContractId = 3,
    /// RegistryKey to ScriptCode index, see [`ScriptCode`](crate::storage::script_code::ScriptCode)
    ScriptCode = 4,
    /// RegistryKey to PredicateCode index, see [`PredicateCode`](crate::storage::predicate_code::PredicateCode)
    PredicateCode = 5,
    /// RegistryKey to ReverseKey index, see [`RegistryIndex`](crate::storage::registry_index::RegistryIndex)
    RegistryIndex = 6,
    /// Keeps track of keys to remove, see [`EvictorCache`](crate::storage::evictor_cache::EvictorCache)
    EvictorCache = 7,
}

impl AsU32 for CompressionColumn {
    fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for CompressionColumn {
    fn name(&self) -> String {
        format!("{:?}", self)
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

/// Type alias to get the `MerkleizedColumn` for some type that implements `MerkleizedTableColumn`.
pub type MerkleizedColumnOf<TC> =
    MerkleizedColumn<<TC as MerkleizedTableColumn>::TableColumn>;
