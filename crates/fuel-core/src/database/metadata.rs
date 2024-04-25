use std::borrow::Cow;

use crate::database::{
    database_description::{
        DatabaseDescription,
        DatabaseMetadata,
    },
    Database,
    Error as DatabaseError,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    structured_storage::TableWithBlueprint,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
};

/// The table that stores all metadata about the database.
pub struct MetadataTable<Description>(core::marker::PhantomData<Description>);

impl<Description> Mappable for MetadataTable<Description>
where
    Description: DatabaseDescription,
{
    type Key = ();
    type OwnedKey = ();
    type Value = DatabaseMetadata<Description::Height>;
    type OwnedValue = Self::Value;
}

impl<Description> TableWithBlueprint for MetadataTable<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Description::Column;

    fn column() -> Self::Column {
        Description::metadata_column()
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
    Self: StorageInspect<MetadataTable<Description>, Error = StorageError>,
{
    /// Reads the metadata from the database.
    fn read_metadata(
        &self,
    ) -> StorageResult<Option<Cow<'_, DatabaseMetadata<Description::Height>>>> {
        self.storage::<MetadataTable<Description>>().get(&())
    }

    /// Ensures the version is correct.
    pub fn check_version(&self) -> StorageResult<()> {
        if let Some(metadata) = self.read_metadata()? {
            if metadata.version() != Description::version() {
                return Err(DatabaseError::InvalidDatabaseVersion {
                    found: metadata.version(),
                    expected: Description::version(),
                }
                .into());
            }
        }
        Ok(())
    }

    /// Reads latest height from the underlying database, bypassing cache
    pub fn latest_height(&self) -> StorageResult<Option<Description::Height>> {
        Ok(self.read_metadata()?.map(|m| *m.height()))
    }

    /// Reads latest genesis active state from the underlying database, bypassing cache
    pub fn genesis_active(&self) -> StorageResult<bool> {
        Ok(self.read_metadata()?.map_or(false, |m| m.genesis_active()))
    }
}
