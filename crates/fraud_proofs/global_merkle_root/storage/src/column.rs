//! The module defines the `Column` and default tables used by the current `fuel-core` codebase.
//! In the future, the `Column` enum should contain only the required tables for the execution.
//! All other tables should live in the downstream creates in the place where they are really used.

#[cfg(feature = "alloc")]
use alloc::string::{
    String,
    ToString,
};
use fuel_core_storage::kv_store::StorageColumn;

/// Almost in the case of all tables we need to prove exclusion of entries,
/// in the case of malicious block.
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
    num_enum::TryFromPrimitive,
)]
pub enum TableColumns {
    /// Only can be proved with global root or a double spend proof.
    ContractsRawCode = 0,
    /// Only can be proved with global root or a double spend proof.
    ContractsLatestUtxo = 1,
    /// Only can be proved with global root or a double spend proof.
    Coins = 2,
    /// Only can be proved with list of events processed during the block
    /// and compared with the `event_inbox_root` in the block header.
    Messages = 3,
    /// We need to prove that the transaction doesn't included into the table.
    /// Only can be proved with global root or a double spend proof.
    ProcessedTransactions = 4,
    /// Only can be proved with global root or a double spend proof.
    ConsensusParametersVersions = 5,
    /// Only can be proved with global root or a double spend proof.
    StateTransitionBytecodeVersions = 6,
    /// Only can be proved with global root or a double spend proof.
    UploadedBytecodes = 7,
    /// Only can be proved with global root or a double spend proof.
    Blobs = 8,
}

impl TableColumns {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

/// Almost in the case of all tables we need to prove exclusion of entries,
/// in the case of malicious block.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Columns {
    TableColumns(TableColumns),
    MerkleMetadataColumns(TableColumns),
    MerkleDataColumns(TableColumns),
}

impl Columns {
    /// The total count of variants in the enum.
    pub const COUNT: usize = TableColumns::COUNT + TableColumns::COUNT;

    /// The start of the merkle metadata columns.
    pub const MERKLE_METADATA_COLUMNS_START: u32 = u16::MAX as u32;

    /// The start of the merkle data columns.
    pub const MERKLE_DATA_COLUMNS_START: u32 =
        Self::MERKLE_METADATA_COLUMNS_START + u16::MAX as u32;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        match self {
            Self::TableColumns(column) => column.as_u32(),
            Self::MerkleMetadataColumns(column) => {
                Self::MERKLE_METADATA_COLUMNS_START + column.as_u32()
            }
            Self::MerkleDataColumns(column) => {
                Self::MERKLE_DATA_COLUMNS_START + column.as_u32()
            }
        }
    }
}

impl StorageColumn for Columns {
    fn name(&self) -> String {
        match self {
            Self::TableColumns(column) => {
                let str: &str = column.into();
                str.to_string()
            }
            Columns::MerkleMetadataColumns(column) => {
                let str: &str = column.into();
                format!("MerkleMetadata{}", str)
            }
            Self::MerkleDataColumns(column) => {
                let str: &str = column.into();
                format!("Merkle{}", str)
            }
        }
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
