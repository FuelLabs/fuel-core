//! The module provides definition and implementation of the relayer storage.

use crate::ports::RelayerDb;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
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

/// Metadata for relayer.
pub struct RelayerMetadata;
impl Mappable for RelayerMetadata {
    type Key = Self::OwnedKey;
    type OwnedKey = ();
    type Value = Self::OwnedValue;
    type OwnedValue = DaBlockHeight;
}

/// Key for da height.
/// If the relayer metadata ever contains more than one key, this should be
/// changed from a unit value.
const METADATA_KEY: () = ();

impl TableWithBlueprint for RelayerMetadata {
    type Blueprint = Plain<Postcard, Primitive<8>>;

    fn column() -> Column {
        Column::RelayerMetadata
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

    fn column() -> Column {
        Column::RelayerHistory
    }
}

impl<T, Storage> RelayerDb for T
where
    T: Send + Sync,
    T: Transactional<Storage = Storage>,
    T: StorageMutate<RelayerMetadata, Error = StorageError>,
    Storage: StorageMutate<EventsHistory, Error = StorageError>
        + StorageMutate<RelayerMetadata, Error = StorageError>,
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
        Ok(*StorageAsRef::storage::<RelayerMetadata>(&self)
            .get(&METADATA_KEY)?
            .unwrap_or_default())
    }
}

fn grow_monotonically<Storage>(
    s: &mut Storage,
    height: &DaBlockHeight,
) -> StorageResult<()>
where
    Storage: StorageMutate<RelayerMetadata, Error = StorageError>,
{
    let current = (&s)
        .storage::<RelayerMetadata>()
        .get(&METADATA_KEY)?
        .map(|cow| cow.as_u64());
    match current {
        Some(current) => {
            if **height > current {
                s.storage::<RelayerMetadata>()
                    .insert(&METADATA_KEY, height)?;
            }
        }
        None => {
            s.storage::<RelayerMetadata>()
                .insert(&METADATA_KEY, height)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        RelayerMetadata,
        <RelayerMetadata as Mappable>::Key::default(),
        <RelayerMetadata as Mappable>::Value::default()
    );

    fuel_core_storage::basic_storage_tests!(
        EventsHistory,
        <EventsHistory as Mappable>::Key::default(),
        vec![Event::Message(Default::default())]
    );
}
