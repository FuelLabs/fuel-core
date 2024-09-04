use fuel_core::database::database_description::DatabaseDescription;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_types::BlockHeight,
};

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
pub enum BenchDbColumn {
    /// See [`Transactions`](crate::tables::Transactions)
    Transactions = 6,
    /// See [`FuelBlocks`](crate::tables::FuelBlocks)
    FuelBlocks = 7,
    FullFuelBlocks = 10902,
    Metadata = 10903,
}

impl StorageColumn for BenchDbColumn {
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        *self as u32
    }
}

/// Full block table. Contains all the information about the block.
pub struct FullFuelBlocks;

impl Mappable for FullFuelBlocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = Block;
}

impl TableWithBlueprint for FullFuelBlocks {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = BenchDbColumn;

    fn column() -> Self::Column {
        Self::Column::FullFuelBlocks
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BenchDatabase;

impl DatabaseDescription for BenchDatabase {
    type Column = BenchDbColumn;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> String {
        "bench_database".to_string()
    }

    fn metadata_column() -> Self::Column {
        Self::Column::Metadata
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}
