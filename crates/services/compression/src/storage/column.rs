//! Column enum for the database.

use fuel_core_storage::{
    kv_store::StorageColumn,
    merkle::column::AsU32,
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
    /// Metadata table
    Metadata = 0,
    /// CompressedBlocks, see [`CompressedBlocks`](crate::storage::compressed_blocks::CompressedBlocks)
    CompressedBlocks = 1,
    /// RegistryKey to Address index, see [`Address`](crate::storage::address::Address)
    Address = 2,
    /// RegistryKey to AssetId index, see [`AssetId`](crate::storage::asset_id::AssetId)
    AssetId = 3,
    /// RegistryKey to ContractId index, see [`ContractId`](crate::storage::contract_id::ContractId)
    ContractId = 4,
    /// RegistryKey to ScriptCode index, see [`ScriptCode`](crate::storage::script_code::ScriptCode)
    ScriptCode = 5,
    /// RegistryKey to PredicateCode index, see [`PredicateCode`](crate::storage::predicate_code::PredicateCode)
    PredicateCode = 6,
    /// RegistryKey to ReverseKey index, see [`RegistryIndex`](crate::storage::registry_index::RegistryIndex)
    RegistryIndex = 7,
    /// Keeps track of keys to remove, see [`EvictorCache`](crate::storage::evictor_cache::EvictorCache)
    EvictorCache = 8,
    /// Keeps track of timestamps, will be removed eventually, see [`Timestamps`](crate::storage::timestamps::Timestamps)
    Timestamps = 9,
    #[cfg(feature = "fault-proving")]
    /// Keeps track of registrations per table associated with a compressed block, see [`Registrations`](crate::storage::registrations::Registrations)
    Registrations = 10,
}

impl AsU32 for CompressionColumn {
    fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for CompressionColumn {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
