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
        + StorageMutate<MetadataTable<Description>>
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
                    version: *version+1,
                    height: *height,
                    balances_indexation_progress: Default::default(),
                };
                info!(
                    "Migrating metadata from V1 to version V2..."
                );
                dbg!(&new_metadata);
                info!("XXXX - 3007");
                let x = self.storage_as_mut::<MetadataTable<Description>>();

                // None of these work.
                //x.insert(&(), &new_metadata)?;
                //x.replace(&(), &new_metadata)?;

                info!(
                    "...Migrated! perhaps"
                );
                Ok(())

                // We probably want to use a pattern similar to this code. Or maybe not, since
                // we're just starting up and no other services are running.
                /*
                let updated_changes = if let Some(new_height) = new_height {
                    // We want to update the metadata table to include a new height.
                    // For that, we are building a new storage transaction around `changes`.
                    // Modifying this transaction will include all required updates into the `changes`.
                    let mut transaction = StorageTransaction::transaction(
                        &database,
                        ConflictPolicy::Overwrite,
                        changes,
                    );
                    transaction
                        .storage_as_mut::<MetadataTable<Description>>()
                        .insert(
                            &(),
                            &DatabaseMetadata::V2 {
                                version: Description::version(),
                                height: new_height,
                                // TODO[RC]: This value must NOT be updated here.
                                balances_indexation_progress: Default::default(),
                            },
                        )?;
            
                    transaction.into_changes()
                } else {
                    changes
                };
                let mut guard = database.stage.height.lock();
                database.data.commit_changes(new_height, updated_changes)?;
                */
            }
            DatabaseMetadata::V2 {
                version,
                height,
                balances_indexation_progress,
            } => return Ok(()),
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
