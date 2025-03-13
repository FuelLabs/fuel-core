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
    strum_macros::EnumCount,
    strum_macros::IntoStaticStr,
)]
pub enum MerkleizedColumn<TC> {
    /// The column with the specific data.
    TableColumn(TC),
    /// The column with the merkle tree nodes.
    MerkleDataColumn(TC),
    /// The merkle metadata column.
    MerkleMetadataColumn,
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
    /// Since we have two columns for each table column and one for the merkle data,
    /// we have to multiply the count of the table columns by 2 and add one for the merkle metadata.
    pub const COUNT: usize = TC::COUNT * 2 + 1;

    /// The start of the merkle data columns.
    pub const MERKLE_DATA_COLUMNS_START: u32 = u16::MAX as u32;

    /// Returns the `u32` representation of the `Column`.
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

impl<TC> StorageColumn for MerkleizedColumn<TC>
where
    TC: core::fmt::Debug + Copy + strum::EnumCount + AsU32,
{
    fn name(&self) -> String {
        match self {
            Self::TableColumn(column) => {
                format!("{:?}", column)
            }
            Self::MerkleDataColumn(column) => {
                format!("Merkle{:?}", column)
            }
            MerkleizedColumn::MerkleMetadataColumn => "MerkleMetadata".to_string(),
        }
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}
