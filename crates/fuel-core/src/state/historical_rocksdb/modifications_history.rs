use crate::{
    database::database_description::DatabaseDescription,
    state::historical_rocksdb::description::Column,
};
use fuel_core_storage::{
    self,
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    structured_storage::TableWithBlueprint,
    transactional::Changes,
    Mappable,
};

/// Versioned modification history. The `const VERSION: usize` generic parameter
/// is used to specify the version.
/// This allows to define different [`TableWithBlueprint`]
/// implementations for V1 and V2 of the modification history.
pub struct ModificationsHistoryVersion<Description, const VERSION: usize>(
    core::marker::PhantomData<Description>,
)
where
    Description: DatabaseDescription;

/// [`ModificationsHistoryVersion`] keys and values for all versions.
impl<Description, const VERSION: usize> Mappable
    for ModificationsHistoryVersion<Description, VERSION>
where
    Description: DatabaseDescription,
{
    /// The height of the modifications.
    type Key = u64;
    type OwnedKey = Self::Key;
    /// Reverse modification at the corresponding height.
    type Value = Changes;
    type OwnedValue = Self::Value;
}

pub type ModificationsHistoryV1<Description> =
    ModificationsHistoryVersion<Description, 0>;
pub type ModificationsHistoryV2<Description> =
    ModificationsHistoryVersion<Description, 1>;

/// Blueprint for Modifications History V1. Keys are stored in little endian order
/// using the `Column::HistoryColumn` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryV1<Description>
where
    Description: DatabaseDescription,
{
    //  the keys in the database. https://github.com/FuelLabs/fuel-core/issues/2095
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column<Description>;

    fn column() -> Self::Column {
        Column::HistoryColumn
    }
}

/// Blueprint for Modifications History V2. Keys are stored in big endian order
/// using the `Column::HistoryColumnV2` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryV2<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column<Description>;

    // We store key-value pairs using the same column family as for V1
    fn column() -> Self::Column {
        Column::HistoryV2Column
    }
}
