//! This module defines a `MerklizedColumn` type which defines data, merkledata and merkle metadata for it
use crate::kv_store::StorageColumn;
use alloc::{
    format,
    string::{
        String,
        ToString,
    },
};

/// Almost in the case of all tables we need to prove exclusion of entries,
/// in the case of malicious block.
#[repr(u32)]
#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    enum_iterator::Sequence,
    strum_macros::IntoStaticStr,
)]
pub enum MerkleizedColumn<TC> {
    /// The Metadata column (non-merkleized)
    Metadata,
    /// The merkleized metadata column.
    MerkleMetadataColumn,
    /// The column with the specific data.
    TableColumn(TC),
    /// The column with the merkle tree nodes.
    MerkleDataColumn(TC),
}

impl<TC> strum::EnumCount for MerkleizedColumn<TC>
where
    TC: strum::EnumCount + AsU32,
{
    /// The total count of variants in the enum.
    /// Since we have two columns for each table column and one for the merkle data,
    /// we have to multiply the count of the table columns by 2 and add 2 for the metadata tables.
    const COUNT: usize = TC::COUNT * 2 + 2;
}

/// The trait to convert the column to the `u32`.
pub trait AsU32 {
    /// Returns the `u32` representation of the `Column`.
    fn as_u32(&self) -> u32;
}

impl<TC> MerkleizedColumn<TC>
where
    TC: strum::EnumCount + AsU32,
{
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// The start of the merkle data columns.
    pub const MERKLE_DATA_COLUMNS_START: u32 = u16::MAX as u32;

    /// The metadata column (non-merkleized)
    pub const METADATA_COLUMN: u32 = 0;

    /// The merkle metadata column
    // we set it to 1, since any future column updates will be added after this column
    pub const MERKLE_METADATA_COLUMN: u32 = 1;

    #[inline]
    fn with_metadata_offset(column: u32) -> u32 {
        column.wrapping_add(Self::MERKLE_METADATA_COLUMN + 1)
    }

    /// Returns the `u32` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        match self {
            Self::Metadata => Self::METADATA_COLUMN,
            Self::MerkleMetadataColumn => Self::MERKLE_METADATA_COLUMN,
            Self::TableColumn(column) => Self::with_metadata_offset(column.as_u32()),
            Self::MerkleDataColumn(column) => Self::with_metadata_offset(
                Self::MERKLE_DATA_COLUMNS_START.wrapping_add(column.as_u32()),
            ),
        }
    }
}

impl<TC> StorageColumn for MerkleizedColumn<TC>
where
    TC: core::fmt::Debug + Copy + strum::EnumCount + AsU32,
{
    fn name(&self) -> String {
        match self {
            Self::Metadata => "Metadata".to_string(),
            MerkleizedColumn::MerkleMetadataColumn => "MerkleMetadata".to_string(),
            Self::TableColumn(column) => {
                format!("{:?}", column)
            }
            Self::MerkleDataColumn(column) => {
                format!("Merkle{:?}", column)
            }
        }
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
