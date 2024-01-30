//! Ports used by the relayer to access the outside world

use async_trait::async_trait;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::Messages,
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
    entities::message::Message,
};

#[cfg(test)]
mod tests;

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: Send + Sync {
    /// Add bridge messages to database. Messages are not revertible.
    /// Must only set a new da height if it is greater than the current.
    fn insert_messages(
        &mut self,
        da_height: &DaBlockHeight,
        messages: &[Message],
    ) -> StorageResult<()>;

    /// Set finalized da height that represent last block from da layer that got finalized.
    /// This will only set the value if it is greater than the current.
    fn set_finalized_da_height_to_at_least(
        &mut self,
        block: &DaBlockHeight,
    ) -> StorageResult<()>;

    /// Get finalized da height that represent last block from da layer that got finalized.
    /// Panics if height is not set as of initialization of database.
    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
}

impl<T, Storage> RelayerDb for T
where
    T: Send + Sync,
    T: Transactional<Storage = Storage>,
    T: StorageMutate<RelayerMetadata, Error = StorageError>,
    Storage: StorageMutate<Messages, Error = StorageError>
        + StorageMutate<RelayerMetadata, Error = StorageError>,
{
    fn insert_messages(
        &mut self,
        da_height: &DaBlockHeight,
        messages: &[Message],
    ) -> StorageResult<()> {
        // A transaction is required to ensure that the height is
        // set atomically with the insertion based on the current
        // height. Also so that the messages are inserted atomically
        // with the height.
        let mut db_tx = self.transaction();
        let db = db_tx.as_mut();

        let mut max_height = None;
        for message in messages {
            db.storage::<Messages>().insert(message.id(), message)?;
            let max = max_height.get_or_insert(0u64);
            *max = (*max).max(message.da_height().0);
        }
        if let Some(height) = max_height {
            if **da_height < height {
                return Err(anyhow::anyhow!("Invalid da height").into())
            }
        }
        grow_monotonically(db, da_height)?;
        db_tx.commit()?;
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

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    RelayerMetadata,
    <RelayerMetadata as Mappable>::Key::default(),
    <RelayerMetadata as Mappable>::Value::default()
);
