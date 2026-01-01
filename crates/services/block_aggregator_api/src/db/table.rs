use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;

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
    LatestBlock = 2,
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
    type OwnedValue = Arc<[u8]>;
}

impl TableWithBlueprint for Blocks {
    type Blueprint = Plain<Primitive<4>, Raw>;
    type Column = Column;

    fn column() -> Self::Column {
        Column::Blocks
    }
}

pub struct LatestBlock;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Mode {
    Local(BlockHeight),
    S3(BlockHeight),
}

impl Mode {
    pub fn new_s3(height: BlockHeight) -> Self {
        Self::S3(height)
    }

    pub fn new_local(height: BlockHeight) -> Self {
        Self::Local(height)
    }

    pub fn height(&self) -> BlockHeight {
        match self {
            Self::Local(height) => *height,
            Self::S3(height) => *height,
        }
    }
}

impl Mappable for LatestBlock {
    type Key = Self::OwnedKey;
    type OwnedKey = ();
    type Value = Self::OwnedValue;
    type OwnedValue = Mode;
}

impl TableWithBlueprint for LatestBlock {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;
    fn column() -> Self::Column {
        Column::LatestBlock
    }
}

use fuel_core_storage::codec::{
    postcard::Postcard,
    primitive::Primitive,
    raw::Raw,
};
