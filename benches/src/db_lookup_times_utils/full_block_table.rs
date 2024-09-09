use fuel_core::database::database_description::DatabaseDescription;
use fuel_core_storage::kv_store::StorageColumn;
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
pub enum BenchDbColumn {
    /// See [`Transactions`](crate::tables::Transactions)
    Transactions = 0,
    /// See [`FuelBlocks`](crate::tables::FuelBlocks)
    FuelBlocks = 1,
    FullFuelBlocks = 2,
    Metadata = 3,
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
