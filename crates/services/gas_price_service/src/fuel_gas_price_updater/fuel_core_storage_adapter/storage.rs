use crate::fuel_gas_price_updater::UpdaterMetadata;
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
pub enum GasPriceColumn {
    Metadata = 0,
    State = 1,
}

impl GasPriceColumn {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for GasPriceColumn {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

/// The storage table for metadata of the gas price algorithm updater
pub struct GasPriceMetadata;

impl Mappable for GasPriceMetadata {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = UpdaterMetadata;
}

impl TableWithBlueprint for GasPriceMetadata {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = GasPriceColumn;

    fn column() -> Self::Column {
        GasPriceColumn::State
    }
}
