use crate::{
    database::database_description::DatabaseDescription,
    state::historical_rocksdb::description::Column,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    transactional::Changes,
    Mappable,
};

pub struct ModificationsHistory<Description>(core::marker::PhantomData<Description>)
where
    Description: DatabaseDescription;

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

impl<Description> TableWithBlueprint for ModificationsHistory<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain;
}

impl<Description> Interlayer for ModificationsHistory<Description>
where
    Description: DatabaseDescription,
{
    type KeyCodec = Postcard;
    type ValueCodec = Postcard;
    type Column = Column<Description>;

    fn column() -> Self::Column {
        Column::HistoryColumn
    }
}
