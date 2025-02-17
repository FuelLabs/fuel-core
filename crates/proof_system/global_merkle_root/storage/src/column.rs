use alloc::{
    format,
    string::{
        String,
        ToString,
    },
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
pub enum TableColumn {
    /// Only can be proved with global root or a double spend proof.
    ContractsRawCode = 0,
    /// Only can be proved with global root or a double spend proof.
    ContractsLatestUtxo = 1,
    /// Only can be proved with global root or a double spend proof.
    Coins = 2,
    /// Only can be proved with list of events processed during the block
    /// and compared with the `event_inbox_root` in the block header.
    Messages = 3,
    /// We need to prove that the transaction isn't included in the table.
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

impl TableColumn {
    /// The total count of variants in the enum.
    const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

/// Almost in the case of all tables we need to prove exclusion of entries,
/// in the case of malicious block.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    /// The column with the specific data.
    TableColumn(TableColumn),
    /// The column with the merkle tree nodes.
    MerkleDataColumn(TableColumn),
    /// The merkle metadata column.
    MerkleMetadataColumn,
}

impl Column {
    /// The total count of variants in the enum.
    pub const COUNT: usize = TableColumn::COUNT + TableColumn::COUNT + 1;

    /// The start of the merkle data columns.
    pub const MERKLE_DATA_COLUMNS_START: u32 = u16::MAX as u32;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        match self {
            Self::TableColumn(column) => column.as_u32(),
            Self::MerkleDataColumn(column) => {
                Self::MERKLE_DATA_COLUMNS_START.wrapping_add(column.as_u32())
            }
            Self::MerkleMetadataColumn => u32::MAX,
        }
    }
}

impl StorageColumn for Column {
    fn name(&self) -> String {
        match self {
            Self::TableColumn(column) => {
                let str: &str = column.into();
                str.to_string()
            }
            Self::MerkleDataColumn(column) => {
                let str: &str = column.into();
                format!("Merkle{}", str)
            }
            Column::MerkleMetadataColumn => "MerkleMetadata".to_string(),
        }
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
