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

/// ModificationsHystory. The `const N: usize` generic parameter
/// is used to specify different versions of the modification history.
/// This allows to define different layout implementations for different
/// versions of the storage history (e.g. by defining different TableWithBlueprint
/// implementations for V1 and V2 of the modification history.
/// The [`ModificationHistoryVersion`]` struct is private. This forces reads and writes
/// to the modification history to go through the [`ModificationHistory`] struct,
/// for which storage inspection and mutation primitives are defined via the [`VersionedStorage`].
pub struct ModificationsHistoryVersion<Description, const N: usize>(
    core::marker::PhantomData<Description>,
)
where
    Description: DatabaseDescription;

pub struct ModificationsHistory<Description>(core::marker::PhantomData<Description>)
where
    Description: DatabaseDescription;

/// ModificationsHistory Key-value pairs.
impl<Description, const N: usize> Mappable for ModificationsHistoryVersion<Description, N>
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

/// ModificationsHistory Key-value pairs.
impl<Description> Mappable for ModificationsHistory<Description>
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

/// Blueprint for Modifications History V1. Keys are stored in little endian
/// using the `Column::HistoryColumn` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 0>
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

/// Blueprint for Modificaations History V2. Keys are stored in little endian
/// using the `Column::HistoryColumnV2` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 1>
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

pub type ModificationsHistoryV1<Description> =
    ModificationsHistoryVersion<Description, 0>;
pub type ModificationsHistoryV2<Description> =
    ModificationsHistoryVersion<Description, 1>;
