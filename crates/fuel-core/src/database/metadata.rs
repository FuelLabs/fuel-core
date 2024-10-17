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
    transactional::{
        Changes,
        ConflictPolicy,
        Modifiable,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
    StorageMutateForced,
};
use tracing::info;

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

impl<Description, Stage> Database<Description, Stage>
where
    Description: DatabaseDescription,
    Self: StorageInspect<MetadataTable<Description>, Error = StorageError>
        + StorageMutateForced<MetadataTable<Description>>
        + Modifiable,
{
    // TODO[RC]: Add test covering this.
    pub fn migrate_metadata(&mut self) -> StorageResult<()> {
        let Some(current_metadata) =
            self.storage::<MetadataTable<Description>>().get(&())?
        else {
            return Ok(());
        };

        dbg!(&current_metadata);

        match current_metadata.as_ref() {
            DatabaseMetadata::V1 { version, height } => {
                let new_metadata = DatabaseMetadata::V2 {
                    version: *version + 1,
                    height: *height,
                    indexation_progress: Default::default(),
                };
                info!("Migrating metadata from V1 to version V2...");
                dbg!(&new_metadata);
                let x = self.storage_as_mut::<MetadataTable<Description>>();
                x.replace_forced(&(), &new_metadata)?;

                info!("...Migrated!");
                Ok(())
            }
            DatabaseMetadata::V2 { .. } => return Ok(()),
        }
    }
}

impl<Description, Stage> Database<Description, Stage>
where
    Description: DatabaseDescription,
    Self: StorageInspect<MetadataTable<Description>, Error = StorageError>,
{
    /// Ensures the version is correct.
    pub fn check_version(&self) -> StorageResult<()> {
        let Some(metadata) = self.storage::<MetadataTable<Description>>().get(&())?
        else {
            return Ok(());
        };

        if metadata.version() != Description::version() {
            return Err(DatabaseError::InvalidDatabaseVersion {
                found: metadata.version(),
                expected: Description::version(),
            }
            .into())
        }

        Ok(())
    }

    pub fn latest_height_from_metadata(
        &self,
    ) -> StorageResult<Option<Description::Height>> {
        let metadata = self.storage::<MetadataTable<Description>>().get(&())?;

        let metadata = metadata.map(|metadata| *metadata.height());

        Ok(metadata)
    }
}
