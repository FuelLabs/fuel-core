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
    /// RegistryKey to Address index
    Address = 0,
    /// RegistryKey to AssetId index
    AssetId = 1,
    /// RegistryKey to ContractId index
    ContractId = 2,
    /// RegistryKey to ScriptCode index
    ScriptCode = 3,
    /// RegistryKey to PredicateCode index
    PredicateCode = 4,
    /// RegistryKey to ReverseKey index
    RegistryIndex = 5,
    /// RegistryKey to Timestamps index
    Timestamps = 6,
    /// Keeps track of keys to remove
    EvictorCache = 7,
    /// CompressedBlocks
    CompressedBlocks = 20,
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
