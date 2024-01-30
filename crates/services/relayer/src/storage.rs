//! The module provides definition and implementation of the relayer storage.

use crate::ports::RelayerDb;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    transactional::Transactional,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

/// GraphQL database tables column ids to the corresponding [`fuel_core_storage::Mappable`] table.
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
)]
pub enum Column {
    /// The column id of metadata about the relayer storage.
    Metadata = 0,
    /// The column of the table that stores history of the relayer.
    History = 1,
    /// The column that tracks the da height of the relayer.
    RelayerHeight = 2,
}

impl Column {
    /// The total count of variants in the enum.
    pub const COUNT: usize = <Self as strum::EnumCount>::COUNT;

    /// Returns the `usize` representation of the `Column`.
    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl StorageColumn for Column {
    fn name(&self) -> &'static str {
        self.into()
    }

    fn id(&self) -> u32 {
        self.as_u32()
    }
}

/// Teh table to track the relayer's da height.
pub struct DaHeightTable;
impl Mappable for DaHeightTable {
    type Key = Self::OwnedKey;
    type OwnedKey = ();
    type Value = Self::OwnedValue;
    type OwnedValue = DaBlockHeight;
}

/// Key for da height.
/// If the relayer metadata ever contains more than one key, this should be
/// changed from a unit value.
const METADATA_KEY: () = ();

impl TableWithBlueprint for DaHeightTable {
    type Blueprint = Plain<Postcard, Primitive<8>>;
    type Column = Column;

    fn column() -> Column {
        Column::RelayerHeight
    }
}

/// The table contains history of events on the DA.
pub struct EventsHistory;

impl Mappable for EventsHistory {
    /// The key is the height of the DA.
    type Key = Self::OwnedKey;
    type OwnedKey = DaBlockHeight;
    /// The value is an events happened at the height.
    type Value = [Event];
    type OwnedValue = Vec<Event>;
}

impl TableWithBlueprint for EventsHistory {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::History
    }
}

impl<T, Storage> RelayerDb for T
where
    T: Send + Sync,
    T: Transactional<Storage = Storage>,
    T: StorageMutate<DaHeightTable, Error = StorageError>,
    Storage: StorageMutate<EventsHistory, Error = StorageError>
        + StorageMutate<DaHeightTable, Error = StorageError>,
{
    fn insert_events(
        &mut self,
        da_height: &DaBlockHeight,
        events: &[Event],
    ) -> StorageResult<()> {
        // A transaction is required to ensure that the height is
        // set atomically with the insertion based on the current
        // height. Also so that the messages are inserted atomically
        // with the height.
        let mut db_tx = self.transaction();
        let db = db_tx.as_mut();

        for event in events {
            if da_height != &event.da_height() {
                return Err(anyhow::anyhow!("Invalid da height").into())
            }
        }

        db.storage::<EventsHistory>().insert(da_height, events)?;

        grow_monotonically(db, da_height)?;
        db_tx.commit()?;
        // TODO: Think later about how to clean up the history of the relayer.
        //  Since we don't have too much information on the relayer and it can be useful
        //  at any time, maybe we want to consider keeping it all the time instead of creating snapshots.
        //  https://github.com/FuelLabs/fuel-core/issues/1627
        Ok(())
    }

    fn set_finalized_da_height_to_at_least(
        &mut self,
        height: &DaBlockHeight,
    ) -> StorageResult<()> {
        // A transaction is required to ensure that the height is
        // set atomically with the insertion based on the current
        // height.
        let mut db_tx = self.transaction();
        let db = db_tx.as_mut();
        grow_monotonically(db, height)?;
        db_tx.commit()?;
        Ok(())
    }

    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight> {
        Ok(*StorageAsRef::storage::<DaHeightTable>(&self)
            .get(&METADATA_KEY)?
            .unwrap_or_default())
    }
}

fn grow_monotonically<Storage>(
    s: &mut Storage,
    height: &DaBlockHeight,
) -> StorageResult<()>
where
    Storage: StorageMutate<DaHeightTable, Error = StorageError>,
{
    let current = (&s)
        .storage::<DaHeightTable>()
        .get(&METADATA_KEY)?
        .map(|cow| cow.as_u64());
    match current {
        Some(current) => {
            if **height > current {
                s.storage::<DaHeightTable>().insert(&METADATA_KEY, height)?;
            }
        }
        None => {
            s.storage::<DaHeightTable>().insert(&METADATA_KEY, height)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        DaHeightTable,
        <DaHeightTable as Mappable>::Key::default(),
        <DaHeightTable as Mappable>::Value::default()
    );

    fuel_core_storage::basic_storage_tests!(
        EventsHistory,
        <EventsHistory as Mappable>::Key::default(),
        vec![Event::Message(Default::default())]
    );
}
