use crate::blocks::Block;
use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
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
pub enum Column {
    Metadata = 0,
    Blocks = 1,
}

impl Column {
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for Column {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

pub struct Blocks;

impl Mappable for Blocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = Block;
}

impl TableWithBlueprint for Blocks {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Column::Blocks
    }
}
