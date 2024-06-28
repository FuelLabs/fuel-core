use crate::database::{
    commit_changes_with_height_update,
    database_description::DatabaseDescription,
    Database,
};
use fuel_core_gas_price_service::fuel_gas_price_updater::UpdaterMetadata;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    transactional::{
        Changes,
        Modifiable,
    },
    Mappable,
    Result as StorageResult,
};
use fuel_core_types::fuel_types::BlockHeight;
use itertools::Itertools;

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

#[derive(Clone, Debug)]
pub struct GasPrice;

impl DatabaseDescription for GasPrice {
    type Column = GasPriceColumn;
    type Height = BlockHeight;

    fn version() -> u32 {
        0
    }

    fn name() -> &'static str {
        "gas_price_service_storage"
    }

    fn metadata_column() -> Self::Column {
        GasPriceColumn::Metadata
    }

    fn prefix(_column: &Self::Column) -> Option<usize> {
        None
    }
}

impl StorageColumn for GasPriceColumn {
    fn name(&self) -> &'static str {
        self.into()
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

impl Modifiable for Database<GasPrice> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all::<GasPriceMetadata>(Some(IterDirection::Reverse))
                .map(|result| result.map(|(height, _)| height))
                .try_collect()
        })
    }
}
