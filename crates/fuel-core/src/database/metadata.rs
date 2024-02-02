use crate::{
    database::{
        database_description::{
            DatabaseDescription,
            DatabaseMetadata,
        },
        storage::UseStructuredImplementation,
        Database,
        Error as DatabaseError,
    },
    state::DataSource,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    not_found,
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::fuel_merkle::storage::StorageMutate;

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

impl<Description> UseStructuredImplementation<MetadataTable<Description>>
    for StructuredStorage<DataSource<Description>>
where
    Description: DatabaseDescription,
{
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
    Self: StorageMutate<MetadataTable<Description>, Error = StorageError>,
{
    /// Ensures the database is initialized and that the database version is correct
    pub fn init(&mut self, height: &Description::Height) -> StorageResult<()> {
        use fuel_core_storage::StorageAsMut;

        if !self
            .storage::<MetadataTable<Description>>()
            .contains_key(&())?
        {
            let old = self.storage::<MetadataTable<Description>>().insert(
                &(),
                &DatabaseMetadata::V1 {
                    version: Description::version(),
                    height: *height,
                },
            )?;

            if old.is_some() {
                return Err(DatabaseError::ChainAlreadyInitialized.into())
            }
        }

        let metadata = self
            .storage::<MetadataTable<Description>>()
            .get(&())?
            .expect("We checked its existence above");

        if metadata.version() != Description::version() {
            return Err(DatabaseError::InvalidDatabaseVersion {
                found: metadata.version(),
                expected: Description::version(),
            }
            .into())
        }

        Ok(())
    }

    pub fn latest_height(&self) -> StorageResult<Description::Height> {
        let metadata = self.storage::<MetadataTable<Description>>().get(&())?;

        let metadata = metadata.ok_or(not_found!(MetadataTable<Description>))?;

        Ok(*metadata.height())
    }
}
