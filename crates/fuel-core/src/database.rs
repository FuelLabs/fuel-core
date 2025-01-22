use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
            DatabaseDescription,
            DatabaseHeight,
            DatabaseMetadata,
        },
        metadata::MetadataTable,
        Error as DatabaseError,
    },
    graphql_api::storage::blocks::FuelBlockIdsToHeights,
    state::{
        data_source::{
            DataSource,
            DataSourceType,
        },
        generic_database::GenericDatabase,
        in_memory::memory_store::MemoryStore,
        ColumnType,
        IterableKeyValueView,
        KeyValueView,
    },
};
use fuel_core_chain_config::TableEntry;
use fuel_core_gas_price_service::common::fuel_core_storage_adapter::storage::GasPriceMetadata;
use fuel_core_services::SharedMutex;
use fuel_core_storage::{
    self,
    iter::{
        changes_iterator::ChangesIterator,
        IterDirection,
        IterableTable,
        IteratorOverTable,
    },
    not_found,
    tables::FuelBlocks,
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        HistoricalView,
        Modifiable,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use std::{
    borrow::Cow,
    fmt::Debug,
    sync::Arc,
};

pub use fuel_core_database::Error;
pub type Result<T> = core::result::Result<T, Error>;

// TODO: Extract `Database` and all belongs into `fuel-core-database`.
use crate::database::database_description::{
    gas_price::GasPriceDatabase,
    indexation_availability,
};
#[cfg(feature = "rocksdb")]
use crate::state::{
    historical_rocksdb::{
        description::Historical,
        HistoricalRocksDB,
        StateRewindPolicy,
    },
    rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
        RocksDb,
    },
};
#[cfg(feature = "rocksdb")]
use std::path::Path;

// Storages implementation
pub mod balances;
pub mod block;
pub mod coin;
pub mod contracts;
pub mod database_description;
pub mod genesis_progress;
pub mod message;
pub mod metadata;
pub mod sealed_block;
pub mod state;
#[cfg(feature = "test-helpers")]
pub mod storage;
pub mod transactions;

#[derive(Default, Debug, Copy, Clone)]
pub struct GenesisStage;

#[derive(Debug, Clone)]
pub struct RegularStage<Description>
where
    Description: DatabaseDescription,
{
    /// Cached value from Metadata table, used to speed up lookups.
    height: SharedMutex<Option<Description::Height>>,
}

impl<Description> Default for RegularStage<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        Self {
            height: SharedMutex::new(None),
        }
    }
}

pub type Database<Description = OnChain, Stage = RegularStage<Description>> =
    GenericDatabase<DataSource<Description, Stage>>;
pub type OnChainIterableKeyValueView = IterableKeyValueView<ColumnType<OnChain>>;
pub type OffChainIterableKeyValueView = IterableKeyValueView<ColumnType<OffChain>>;
pub type RelayerIterableKeyValueView = IterableKeyValueView<ColumnType<Relayer>>;

pub type GenesisDatabase<Description = OnChain> = Database<Description, GenesisStage>;

impl OnChainIterableKeyValueView {
    pub fn maybe_latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        self.iter_all_keys::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()
    }

    pub fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.maybe_latest_height()?.ok_or(not_found!("BlockHeight"))
    }

    pub fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?
            .map(|(_, block)| block)
            .ok_or_else(|| not_found!("FuelBlocks"))
    }
}

impl<DbDesc> Database<DbDesc>
where
    DbDesc: DatabaseDescription,
{
    pub fn entries<'a, T>(
        &'a self,
        prefix: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<TableEntry<T>>> + 'a
    where
        T: Mappable + 'a,
        Self: IterableTable<T>,
    {
        self.iter_all_filtered::<T, _>(prefix, None, Some(direction))
            .map_ok(|(key, value)| TableEntry { key, value })
    }
}

impl<Description> GenesisDatabase<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(data_source: DataSourceType<Description>) -> Self {
        GenesisDatabase::from_storage(DataSource::new(data_source, GenesisStage))
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
    Database<Description>:
        StorageInspect<MetadataTable<Description>, Error = StorageError>,
{
    pub fn new(data_source: DataSourceType<Description>) -> Self {
        let mut database = Self::from_storage(DataSource::new(
            data_source,
            RegularStage {
                height: SharedMutex::new(None),
            },
        ));
        let height = database
            .latest_height_from_metadata()
            .expect("Failed to get latest height during creation of the database");

        database.stage.height = SharedMutex::new(height);

        database
    }

    #[cfg(feature = "rocksdb")]
    pub fn open_rocksdb(
        path: &Path,
        state_rewind_policy: StateRewindPolicy,
        database_config: DatabaseConfig,
    ) -> Result<Self> {
        use anyhow::Context;

        let db = HistoricalRocksDB::<Description>::default_open(
            path,
            state_rewind_policy,
            database_config,
        )
        .map_err(Into::<anyhow::Error>::into)
        .with_context(|| {
            format!(
                "Failed to open rocksdb, you may need to wipe a \
                pre-existing incompatible db e.g. `rm -rf {path:?}`"
            )
        })?;

        Ok(Self::new(Arc::new(db)))
    }

    /// Converts the regular database to an unchecked database.
    ///
    /// Returns an error in the case regular database is initialized with the `GenesisDatabase`,
    /// to highlight that it is a bad idea and it is unsafe.
    pub fn into_genesis(
        self,
    ) -> core::result::Result<GenesisDatabase<Description>, GenesisDatabase<Description>>
    {
        if !self.stage.height.lock().is_some() {
            Ok(GenesisDatabase::new(self.into_inner().data))
        } else {
            tracing::warn!(
                "Converting regular database into genesis, \
                while height is already set for `{}`",
                Description::name()
            );
            Err(GenesisDatabase::new(self.into_inner().data))
        }
    }
}

impl<Description, Stage> Database<Description, Stage>
where
    Description: DatabaseDescription,
    Stage: Default,
{
    pub fn in_memory() -> Self {
        let data = Arc::<MemoryStore<Description>>::new(MemoryStore::default());
        Self::from_storage(DataSource::new(data, Stage::default()))
    }

    #[cfg(feature = "rocksdb")]
    pub fn rocksdb_temp(
        state_rewind_policy: StateRewindPolicy,
        database_config: DatabaseConfig,
    ) -> Result<Self> {
        let db = RocksDb::<Historical<Description>>::default_open_temp_with_params(
            database_config,
        )?;
        let historical_db = HistoricalRocksDB::new(db, state_rewind_policy)?;
        let data = Arc::new(historical_db);
        Ok(Self::from_storage(DataSource::new(data, Stage::default())))
    }
}

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl<Description, Stage> Default for Database<Description, Stage>
where
    Description: DatabaseDescription,
    Stage: Default,
{
    fn default() -> Self {
        #[cfg(not(feature = "rocksdb"))]
        {
            Self::in_memory()
        }
        #[cfg(feature = "rocksdb")]
        {
            Self::rocksdb_temp(
                StateRewindPolicy::NoRewind,
                DatabaseConfig {
                    cache_capacity: None,
                    max_fds: 512,
                    columns_policy: ColumnsPolicy::Lazy,
                },
            )
            .expect("Failed to create a temporary database")
        }
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub fn rollback_last_block(&self) -> StorageResult<()> {
        let mut lock = self.inner_storage().stage.height.lock();
        let height = *lock;

        let Some(height) = height else {
            return Err(
                anyhow::anyhow!("Database doesn't have a height to rollback").into(),
            );
        };
        self.inner_storage().data.rollback_block_to(&height)?;
        let new_height = height.rollback_height();
        *lock = new_height;
        tracing::info!(
            "Rollback of the {} to the height {:?} was successful",
            Description::name(),
            new_height
        );

        Ok(())
    }
}

impl<Description> AtomicView for Database<Description>
where
    Description: DatabaseDescription,
{
    type LatestView = IterableKeyValueView<ColumnType<Description>>;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        self.inner_storage().data.latest_view()
    }
}

impl<Description> HistoricalView for Database<Description>
where
    Description: DatabaseDescription,
{
    type Height = Description::Height;
    type ViewAtHeight = KeyValueView<ColumnType<Description>>;

    fn latest_height(&self) -> Option<Self::Height> {
        *self.inner_storage().stage.height.lock()
    }

    fn view_at(&self, height: &Self::Height) -> StorageResult<Self::ViewAtHeight> {
        let lock = self.inner_storage().stage.height.lock();

        match *lock {
            None => return self.latest_view().map(|view| view.into_key_value_view()),
            Some(current_height) if &current_height == height => {
                return self.latest_view().map(|view| view.into_key_value_view())
            }
            _ => {}
        };

        self.inner_storage().data.view_at_height(height)
    }
}

impl Modifiable for Database<OnChain> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all_keys::<FuelBlocks>(Some(IterDirection::Reverse))
                .try_collect()
        })
    }
}

impl Modifiable for Database<OffChain> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all::<FuelBlockIdsToHeights>(Some(IterDirection::Reverse))
                .map(|result| result.map(|(_, height)| height))
                .try_collect()
        })
    }
}

impl Modifiable for Database<GasPriceDatabase> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all_keys::<GasPriceMetadata>(Some(IterDirection::Reverse))
                .try_collect()
        })
    }
}

#[cfg(feature = "relayer")]
impl Modifiable for Database<Relayer> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all_keys::<fuel_core_relayer::storage::EventsHistory>(Some(
                IterDirection::Reverse,
            ))
            .try_collect()
        })
    }
}

#[cfg(not(feature = "relayer"))]
impl Modifiable for Database<Relayer> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |_| Ok(vec![]))
    }
}

impl Modifiable for GenesisDatabase<OnChain> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.data.as_ref().commit_changes(None, changes)
    }
}

impl Modifiable for GenesisDatabase<OffChain> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.data.as_ref().commit_changes(None, changes)
    }
}

impl Modifiable for GenesisDatabase<Relayer> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.data.as_ref().commit_changes(None, changes)
    }
}

pub fn commit_changes_with_height_update<Description>(
    database: &mut Database<Description>,
    changes: Changes,
    heights_lookup: impl Fn(
        &ChangesIterator<Description::Column>,
    ) -> StorageResult<Vec<Description::Height>>,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
    Description::Height: Debug + PartialOrd + DatabaseHeight,
    for<'a> StorageTransaction<&'a &'a mut Database<Description>>:
        StorageMutate<MetadataTable<Description>, Error = StorageError>,
{
    // Gets the all new heights from the `changes`
    let iterator = ChangesIterator::<Description::Column>::new(&changes);
    let new_heights = heights_lookup(&iterator)?;

    // Changes for each block should be committed separately.
    // If we have more than one height, it means we are mixing commits
    // for several heights in one batch - return error in this case.
    if new_heights.len() > 1 {
        return Err(DatabaseError::MultipleHeightsInCommit {
            heights: new_heights.iter().map(DatabaseHeight::as_u64).collect(),
        }
        .into());
    }

    let new_height = new_heights.into_iter().last();
    let prev_height = *database.stage.height.lock();

    match (prev_height, new_height) {
        (None, None) => {
            // We are inside the regenesis process if the old and new heights are not set.
            // In this case, we continue to commit until we discover a new height.
            // This height will be the start of the database.
        }
        (Some(prev_height), Some(new_height)) => {
            // Each new commit should be linked to the previous commit to create a monotonically growing database.

            let next_expected_height = prev_height
                .advance_height()
                .ok_or(DatabaseError::FailedToAdvanceHeight)?;

            if next_expected_height != new_height {
                return Err(DatabaseError::HeightsAreNotLinked {
                    prev_height: prev_height.as_u64(),
                    new_height: new_height.as_u64(),
                }
                .into());
            }
        }
        (None, Some(_)) => {
            // The new height is finally found; starting at this point,
            // all next commits should be linked(the height should increase each time by one).
        }
        (Some(prev_height), None) => {
            // In production, we shouldn't have cases where we call `commit_changes` with intermediate changes.
            // The commit always should contain all data for the corresponding height.
            return Err(DatabaseError::NewHeightIsNotSet {
                prev_height: prev_height.as_u64(),
            }
            .into());
        }
    };

    let updated_changes = if let Some(new_height) = new_height {
        // We want to update the metadata table to include a new height.
        // For that, we are building a new storage transaction around `changes`.
        // Modifying this transaction will include all required updates into the `changes`.
        let mut transaction = StorageTransaction::transaction(
            &database,
            ConflictPolicy::Overwrite,
            changes,
        );
        let maybe_current_metadata = transaction
            .storage_as_mut::<MetadataTable<Description>>()
            .get(&())?;
        let metadata = update_metadata::<Description>(maybe_current_metadata, new_height);
        transaction
            .storage_as_mut::<MetadataTable<Description>>()
            .insert(&(), &metadata)?;

        transaction.into_changes()
    } else {
        changes
    };

    // Atomically commit the changes to the database, and to the mutex-protected field.
    let mut guard = database.stage.height.lock();
    database.data.commit_changes(new_height, updated_changes)?;

    // Update the block height
    if let Some(new_height) = new_height {
        *guard = Some(new_height);
    }

    Ok(())
}

fn update_metadata<Description>(
    maybe_current_metadata: Option<
        Cow<DatabaseMetadata<<Description as DatabaseDescription>::Height>>,
    >,
    new_height: <Description as DatabaseDescription>::Height,
) -> DatabaseMetadata<<Description as DatabaseDescription>::Height>
where
    Description: DatabaseDescription,
{
    let updated_metadata = match maybe_current_metadata.as_ref() {
        Some(metadata) => match metadata.as_ref() {
            DatabaseMetadata::V1 { .. } => DatabaseMetadata::V1 {
                version: Description::version(),
                height: new_height,
            },
            DatabaseMetadata::V2 {
                indexation_availability,
                ..
            } => DatabaseMetadata::V2 {
                version: Description::version(),
                height: new_height,
                indexation_availability: indexation_availability.clone(),
            },
        },
        None => DatabaseMetadata::V2 {
            version: Description::version(),
            height: new_height,
            indexation_availability: indexation_availability::<Description>(None),
        },
    };
    updated_metadata
}

#[cfg(feature = "rocksdb")]
pub fn convert_to_rocksdb_direction(direction: IterDirection) -> rocksdb::Direction {
    match direction {
        IterDirection::Forward => rocksdb::Direction::Forward,
        IterDirection::Reverse => rocksdb::Direction::Reverse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{
        database_description::DatabaseDescription,
        Database,
    };

    fn column_keys_not_exceed_count<Description>()
    where
        Description: DatabaseDescription,
    {
        use enum_iterator::all;
        use fuel_core_storage::kv_store::StorageColumn;
        use strum::EnumCount;
        for column in all::<Description::Column>() {
            assert!(column.as_usize() < Description::Column::COUNT);
        }
    }

    mod on_chain {
        use super::*;
        use crate::database::{
            database_description::on_chain::OnChain,
            DatabaseHeight,
        };
        use fuel_core_storage::{
            tables::Coins,
            transactional::WriteTransaction,
        };
        use fuel_core_types::{
            blockchain::block::CompressedBlock,
            entities::coins::coin::CompressedCoin,
            fuel_tx::UtxoId,
        };

        #[test]
        fn column_keys_not_exceed_count_test() {
            column_keys_not_exceed_count::<OnChain>();
        }

        #[test]
        fn database_advances_with_a_new_block() {
            // Given
            let mut database = Database::<OnChain>::default();
            assert_eq!(database.latest_height(), None);

            // When
            let advanced_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&advanced_height, &CompressedBlock::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<OnChain>::default();
            assert_eq!(database.latest_height(), None);

            // When
            database
                .storage_as_mut::<Coins>()
                .insert(&UtxoId::default(), &CompressedCoin::default())
                .unwrap();

            // Then
            assert_eq!(HistoricalView::latest_height(&database), None);
        }

        #[test]
        fn database_advances_with_linked_blocks() {
            // Given
            let mut database = Database::<OnChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&starting_height, &CompressedBlock::default())
                .unwrap();
            assert_eq!(database.latest_height(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&next_height, &CompressedBlock::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(next_height));
        }

        #[test]
        fn database_fails_with_unlinked_blocks() {
            // Given
            let mut database = Database::<OnChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&starting_height, &CompressedBlock::default())
                .unwrap();

            // When
            let prev_height = 0.into();
            let result = database
                .storage_as_mut::<FuelBlocks>()
                .insert(&prev_height, &CompressedBlock::default());

            // Then
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::HeightsAreNotLinked {
                    prev_height: 1,
                    new_height: 0
                })
                .to_string()
            );
        }

        #[test]
        fn database_fails_with_non_advancing_commit() {
            // Given
            let mut database = Database::<OnChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&starting_height, &CompressedBlock::default())
                .unwrap();

            // When
            let result = database
                .storage_as_mut::<Coins>()
                .insert(&UtxoId::default(), &CompressedCoin::default());

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::NewHeightIsNotSet { prev_height: 1 })
                    .to_string()
            );
        }

        #[test]
        fn database_fails_when_commit_with_several_blocks() {
            let mut database = Database::<OnChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&starting_height, &CompressedBlock::default())
                .unwrap();

            // Given
            let mut transaction = database.write_transaction();
            let next_height = starting_height.advance_height().unwrap();
            let next_next_height = next_height.advance_height().unwrap();
            transaction
                .storage_as_mut::<FuelBlocks>()
                .insert(&next_height, &CompressedBlock::default())
                .unwrap();
            transaction
                .storage_as_mut::<FuelBlocks>()
                .insert(&next_next_height, &CompressedBlock::default())
                .unwrap();

            // When
            let result = transaction.commit();

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::MultipleHeightsInCommit {
                    heights: vec![3, 2]
                })
                .to_string()
            );
        }
    }

    mod off_chain {
        use super::*;
        use crate::{
            database::{
                database_description::off_chain::OffChain,
                DatabaseHeight,
            },
            fuel_core_graphql_api::storage::messages::OwnedMessageKey,
            graphql_api::storage::messages::OwnedMessageIds,
        };
        use fuel_core_storage::transactional::WriteTransaction;

        #[test]
        fn column_keys_not_exceed_count_test() {
            column_keys_not_exceed_count::<OffChain>();
        }

        #[test]
        fn database_advances_with_a_new_block() {
            // Given
            let mut database = Database::<OffChain>::default();
            assert_eq!(database.latest_height(), None);

            // When
            let advanced_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &advanced_height)
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<OffChain>::default();
            assert_eq!(database.latest_height(), None);

            // When
            database
                .storage_as_mut::<OwnedMessageIds>()
                .insert(&OwnedMessageKey::default(), &())
                .unwrap();

            // Then
            assert_eq!(HistoricalView::latest_height(&database), None);
        }

        #[test]
        fn database_advances_with_linked_blocks() {
            // Given
            let mut database = Database::<OffChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &starting_height)
                .unwrap();
            assert_eq!(database.latest_height(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &next_height)
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(next_height));
        }

        #[test]
        fn database_fails_with_unlinked_blocks() {
            // Given
            let mut database = Database::<OffChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &starting_height)
                .unwrap();

            // When
            let prev_height = 0.into();
            let result = database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &prev_height);

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::HeightsAreNotLinked {
                    prev_height: 1,
                    new_height: 0
                })
                .to_string()
            );
        }

        #[test]
        fn database_fails_with_non_advancing_commit() {
            // Given
            let mut database = Database::<OffChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &starting_height)
                .unwrap();

            // When
            let result = database
                .storage_as_mut::<OwnedMessageIds>()
                .insert(&OwnedMessageKey::default(), &());

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::NewHeightIsNotSet { prev_height: 1 })
                    .to_string()
            );
        }

        #[test]
        fn database_fails_when_commit_with_several_blocks() {
            let mut database = Database::<OffChain>::default();
            let starting_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &starting_height)
                .unwrap();

            // Given
            let mut transaction = database.write_transaction();
            let next_height = starting_height.advance_height().unwrap();
            let next_next_height = next_height.advance_height().unwrap();
            transaction
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&[1; 32].into(), &next_height)
                .unwrap();
            transaction
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&[2; 32].into(), &next_next_height)
                .unwrap();

            // When
            let result = transaction.commit();

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::MultipleHeightsInCommit {
                    heights: vec![3, 2]
                })
                .to_string()
            );
        }
    }

    #[cfg(feature = "relayer")]
    mod relayer {
        use super::*;
        use crate::database::{
            database_description::relayer::Relayer,
            DatabaseHeight,
        };
        use fuel_core_relayer::storage::EventsHistory;
        use fuel_core_storage::transactional::WriteTransaction;
        use fuel_core_types::blockchain::primitives::DaBlockHeight;

        #[test]
        fn column_keys_not_exceed_count_test() {
            column_keys_not_exceed_count::<Relayer>();
        }

        #[test]
        fn database_advances_with_a_new_block() {
            // Given
            let mut database = Database::<Relayer>::default();
            assert_eq!(database.latest_height(), None);

            // When
            let advanced_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&advanced_height, &[])
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<Relayer>::default();
            assert_eq!(database.latest_height(), None);

            // When
            database
                .storage_as_mut::<MetadataTable<Relayer>>()
                .insert(
                    &(),
                    &DatabaseMetadata::<DaBlockHeight>::V1 {
                        version: Default::default(),
                        height: Default::default(),
                    },
                )
                .unwrap();

            // Then
            assert_eq!(HistoricalView::latest_height(&database), None);
        }

        #[test]
        fn database_advances_with_linked_blocks() {
            // Given
            let mut database = Database::<Relayer>::default();
            let starting_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&starting_height, &[])
                .unwrap();
            assert_eq!(database.latest_height(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&next_height, &[])
                .unwrap();

            // Then
            assert_eq!(database.latest_height(), Some(next_height));
        }

        #[test]
        fn database_fails_with_unlinked_blocks() {
            // Given
            let mut database = Database::<Relayer>::default();
            let starting_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&starting_height, &[])
                .unwrap();

            // When
            let prev_height = 0u64.into();
            let result = database
                .storage_as_mut::<EventsHistory>()
                .insert(&prev_height, &[]);

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::HeightsAreNotLinked {
                    prev_height: 1,
                    new_height: 0
                })
                .to_string()
            );
        }

        #[test]
        fn database_fails_with_non_advancing_commit() {
            // Given
            let mut database = Database::<Relayer>::default();
            let starting_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&starting_height, &[])
                .unwrap();

            // When
            let result = database.storage_as_mut::<MetadataTable<Relayer>>().insert(
                &(),
                &DatabaseMetadata::<DaBlockHeight>::V1 {
                    version: Default::default(),
                    height: Default::default(),
                },
            );

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::NewHeightIsNotSet { prev_height: 1 })
                    .to_string()
            );
        }

        #[test]
        fn database_fails_when_commit_with_several_blocks() {
            let mut database = Database::<Relayer>::default();
            let starting_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&starting_height, &[])
                .unwrap();

            // Given
            let mut transaction = database.write_transaction();
            let next_height = starting_height.advance_height().unwrap();
            let next_next_height = next_height.advance_height().unwrap();
            transaction
                .storage_as_mut::<EventsHistory>()
                .insert(&next_height, &[])
                .unwrap();
            transaction
                .storage_as_mut::<EventsHistory>()
                .insert(&next_next_height, &[])
                .unwrap();

            // When
            let result = transaction.commit();

            // Then
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().to_string(),
                StorageError::from(DatabaseError::MultipleHeightsInCommit {
                    heights: vec![3, 2]
                })
                .to_string()
            );
        }
    }

    #[cfg(feature = "rocksdb")]
    #[test]
    fn database_iter_all_by_prefix_works() {
        use fuel_core_storage::tables::ContractsRawCode;
        use fuel_core_types::fuel_types::ContractId;
        use std::str::FromStr;

        let test = |mut db: Database<OnChain>| {
            let contract_id_1 = ContractId::from_str(
                "5962be5ebddc516cb4ed7d7e76365f59e0d231ac25b53f262119edf76564aab4",
            )
            .unwrap();

            let mut insert_empty_code = |id| {
                StorageMutate::<ContractsRawCode>::insert(&mut db, &id, &[]).unwrap()
            };
            insert_empty_code(contract_id_1);

            let contract_id_2 = ContractId::from_str(
                "5baf0dcae7c114f647f6e71f1723f59bcfc14ecb28071e74895d97b14873c5dc",
            )
            .unwrap();
            insert_empty_code(contract_id_2);

            let matched_keys: Vec<_> = db
                .iter_all_by_prefix::<ContractsRawCode, _>(Some(contract_id_1))
                .map_ok(|(k, _)| k)
                .try_collect()
                .unwrap();

            assert_eq!(matched_keys, vec![contract_id_1]);
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let db = Database::<OnChain>::in_memory();
        // in memory passes
        test(db);

        let db = Database::<OnChain>::open_rocksdb(
            temp_dir.path(),
            Default::default(),
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        // rocks db fails
        test(db);
    }

    mod metadata {
        use crate::database::database_description::IndexationKind;
        use fuel_core_storage::kv_store::StorageColumn;
        use std::{
            borrow::Cow,
            collections::HashSet,
        };
        use strum::EnumCount;

        use super::{
            database_description::DatabaseDescription,
            update_metadata,
            DatabaseHeight,
            DatabaseMetadata,
        };

        #[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
        struct HeightMock(u64);
        impl DatabaseHeight for HeightMock {
            fn as_u64(&self) -> u64 {
                1
            }

            fn advance_height(&self) -> Option<Self> {
                None
            }

            fn rollback_height(&self) -> Option<Self> {
                None
            }
        }

        const MOCK_VERSION: u32 = 0;

        #[derive(EnumCount, enum_iterator::Sequence, Debug, Clone, Copy)]
        enum ColumnMock {
            Column1,
        }

        impl StorageColumn for ColumnMock {
            fn name(&self) -> String {
                "column".to_string()
            }

            fn id(&self) -> u32 {
                42
            }
        }

        #[derive(Debug, Clone, Copy)]
        struct DatabaseDescriptionMock;
        impl DatabaseDescription for DatabaseDescriptionMock {
            type Column = ColumnMock;

            type Height = HeightMock;

            fn version() -> u32 {
                MOCK_VERSION
            }

            fn name() -> String {
                "mock".to_string()
            }

            fn metadata_column() -> Self::Column {
                Self::Column::Column1
            }

            fn prefix(_: &Self::Column) -> Option<usize> {
                None
            }
        }

        #[test]
        fn update_metadata_preserves_v1() {
            let current_metadata: DatabaseMetadata<HeightMock> = DatabaseMetadata::V1 {
                version: MOCK_VERSION,
                height: HeightMock(1),
            };
            let new_metadata = update_metadata::<DatabaseDescriptionMock>(
                Some(Cow::Borrowed(&current_metadata)),
                HeightMock(2),
            );

            match new_metadata {
                DatabaseMetadata::V1 { version, height } => {
                    assert_eq!(version, current_metadata.version());
                    assert_eq!(height, HeightMock(2));
                }
                DatabaseMetadata::V2 { .. } => panic!("should be V1"),
            }
        }

        #[test]
        fn update_metadata_preserves_v2() {
            let available_indexation = HashSet::new();

            let current_metadata: DatabaseMetadata<HeightMock> = DatabaseMetadata::V2 {
                version: MOCK_VERSION,
                height: HeightMock(1),
                indexation_availability: available_indexation.clone(),
            };
            let new_metadata = update_metadata::<DatabaseDescriptionMock>(
                Some(Cow::Borrowed(&current_metadata)),
                HeightMock(2),
            );

            match new_metadata {
                DatabaseMetadata::V1 { .. } => panic!("should be V2"),
                DatabaseMetadata::V2 {
                    version,
                    height,
                    indexation_availability,
                } => {
                    assert_eq!(version, current_metadata.version());
                    assert_eq!(height, HeightMock(2));
                    assert_eq!(indexation_availability, available_indexation);
                }
            }
        }

        #[test]
        fn update_metadata_none_becomes_v2() {
            let new_metadata =
                update_metadata::<DatabaseDescriptionMock>(None, HeightMock(2));

            match new_metadata {
                DatabaseMetadata::V1 { .. } => panic!("should be V2"),
                DatabaseMetadata::V2 {
                    version,
                    height,
                    indexation_availability,
                } => {
                    assert_eq!(version, MOCK_VERSION);
                    assert_eq!(height, HeightMock(2));
                    assert_eq!(indexation_availability, IndexationKind::all().collect());
                }
            }
        }
    }
}
