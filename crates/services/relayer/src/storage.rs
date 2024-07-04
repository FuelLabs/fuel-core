//! The module provides definition and implementation of the relayer storage.

use crate::ports::{
    DatabaseTransaction,
    RelayerDb,
    Transactional,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    kv_store::StorageColumn,
    structured_storage::TableWithBlueprint,
    transactional::{
        Modifiable,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
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
    fn name(&self) -> String {
        let str: &str = self.into();
        str.to_string()
    }

    fn id(&self) -> u32 {
        self.as_u32()
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

impl<T> RelayerDb for T
where
    T: Send + Sync,
    T: Transactional,
    for<'a> T::Transaction<'a>: StorageMutate<EventsHistory, Error = StorageError>,
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

        // Get the current DA block height from the database.
        let before = self.latest_da_height().unwrap_or_default();

        let mut db_tx = self.transaction();

        for event in events {
            if da_height != &event.da_height() {
                return Err(anyhow::anyhow!("Invalid da height").into());
            }
        }

        db_tx.storage::<EventsHistory>().insert(da_height, events)?;
        db_tx.commit()?;

        // Compare the new DA block height with previous the block height. Block
        // height must always be monotonically increasing. If the new block
        // height is less than the previous block height, the service is in
        // an error state and must be shut down.
        let after = self
            .latest_da_height()
            .expect("DA height must be set at this point");
        if after < before {
            StorageResult::Err(
                anyhow::anyhow!("Block height must be monotonically increasing").into(),
            )?
        }

        // TODO: Think later about how to clean up the history of the relayer.
        //  Since we don't have too much information on the relayer and it can be useful
        //  at any time, maybe we want to consider keeping it all the time instead of creating snapshots.
        //  https://github.com/FuelLabs/fuel-core/issues/1627
        Ok(())
    }

    fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        self.latest_da_height()
    }
}

impl<S> DatabaseTransaction for StorageTransaction<S>
where
    S: Modifiable,
{
    fn commit(self) -> StorageResult<()> {
        self.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        EventsHistory,
        <EventsHistory as Mappable>::Key::default(),
        vec![
            Event::Message(Default::default()),
            Event::Transaction(Default::default())
        ]
    );
}
