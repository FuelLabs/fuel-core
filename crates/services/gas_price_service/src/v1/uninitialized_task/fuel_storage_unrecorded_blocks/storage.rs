use crate::common::updater_metadata::UpdaterMetadata;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::v1::Height;

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
pub enum UnrecordedBlocksColumn {
    Metadata = 0,
    State = 1,
}

impl UnrecordedBlocksColumn {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for UnrecordedBlocksColumn {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

/// The storage table for metadata of the gas price algorithm updater
pub struct UnrecordedBlocksTable;

impl Mappable for UnrecordedBlocksTable {
    type Key = Self::OwnedKey;
    type OwnedKey = u32;
    type Value = Self::OwnedValue;
    type OwnedValue = u64;
}

impl TableWithBlueprint for UnrecordedBlocksTable {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = UnrecordedBlocksColumn;

    fn column() -> Self::Column {
        UnrecordedBlocksColumn::State
    }
}
