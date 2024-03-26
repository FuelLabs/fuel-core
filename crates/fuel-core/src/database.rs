use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
            DatabaseDescription,
            DatabaseMetadata,
        },
        metadata::MetadataTable,
        Error as DatabaseError,
    },
    graphql_api::storage::blocks::FuelBlockIdsToHeights,
    state::{
        in_memory::memory_store::MemoryStore,
        ChangesIterator,
        DataSource,
    },
};
use fuel_core_chain_config::TableEntry;
use fuel_core_services::SharedMutex;
use fuel_core_storage::{
    self,
    blueprint::BlueprintInspect,
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
        IteratorOverTable,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        Value,
    },
    not_found,
    structured_storage::TableWithBlueprint,
    tables::FuelBlocks,
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        Modifiable,
        StorageTransaction,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::DaBlockHeight,
    },
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use std::{
    fmt::Debug,
    sync::Arc,
};

pub use fuel_core_database::Error;
pub type Result<T> = core::result::Result<T, Error>;

// TODO: Extract `Database` and all belongs into `fuel-core-database`.
#[cfg(feature = "rocksdb")]
use crate::state::rocks_db::RocksDb;
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
pub mod storage;
pub mod transactions;

#[derive(Clone, Debug)]
pub struct Database<Description = OnChain>
where
    Description: DatabaseDescription,
{
    height: SharedMutex<Option<Description::Height>>,
    data: DataSource<Description>,
}

impl Database<OnChain> {
    pub fn latest_block(&self) -> StorageResult<CompressedBlock> {
        self.iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?
            .map(|(_, block)| block)
            .ok_or_else(|| not_found!("FuelBlocks"))
    }

    pub fn entries<'a, T>(
        &'a self,
        prefix: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<TableEntry<T>>> + 'a
    where
        T: TableWithBlueprint<Column = <OnChain as DatabaseDescription>::Column>,
        T::OwnedValue: 'a,
        T::OwnedKey: 'a,
        T::Blueprint: BlueprintInspect<T, Self>,
    {
        self.iter_all_filtered::<T, _>(prefix, None, Some(direction))
            .map_ok(|(key, value)| TableEntry { key, value })
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
    Self: StorageInspect<MetadataTable<Description>, Error = StorageError>,
{
    pub fn new(data_source: DataSource<Description>) -> Self {
        let mut database = Self {
            height: SharedMutex::new(None),
            data: data_source,
        };
        let height = database
            .latest_height()
            .expect("Failed to get latest height during creation of the database");

        database.height = SharedMutex::new(height);

        database
    }

    #[cfg(feature = "rocksdb")]
    pub fn open_rocksdb(path: &Path, capacity: impl Into<Option<usize>>) -> Result<Self> {
        use anyhow::Context;
        let db = RocksDb::<Description>::default_open(path, capacity.into()).map_err(Into::<anyhow::Error>::into).context("Failed to open rocksdb, you may need to wipe a pre-existing incompatible db `rm -rf ~/.fuel/db`")?;

        Ok(Database::new(Arc::new(db)))
    }
}

impl<Description> Database<Description>
where
    Description: DatabaseDescription,
{
    pub fn in_memory() -> Self {
        let data = Arc::<MemoryStore<Description>>::new(MemoryStore::default());
        Self {
            height: SharedMutex::new(None),
            data,
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn rocksdb_temp() -> Self {
        let data =
            Arc::<RocksDb<Description>>::new(RocksDb::default_open_temp(None).unwrap());
        Self {
            height: SharedMutex::new(None),
            data,
        }
    }
}

impl<Description> KeyValueInspect for Database<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.data.as_ref().exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.data.as_ref().size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.data.as_ref().get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.data.as_ref().read(key, column, buf)
    }
}

impl<Description> IterableStore for Database<Description>
where
    Description: DatabaseDescription,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.data
            .as_ref()
            .iter_store(column, prefix, start, direction)
    }
}

/// Construct an ephemeral database
/// uses rocksdb when rocksdb features are enabled
/// uses in-memory when rocksdb features are disabled
impl<Description> Default for Database<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        #[cfg(not(feature = "rocksdb"))]
        {
            Self::in_memory()
        }
        #[cfg(feature = "rocksdb")]
        {
            Self::rocksdb_temp()
        }
    }
}

impl AtomicView for Database<OnChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> Option<Self::Height> {
        *self.height.lock()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<OffChain> {
    type View = Self;

    type Height = BlockHeight;

    fn latest_height(&self) -> Option<Self::Height> {
        *self.height.lock()
    }

    fn view_at(&self, _: &BlockHeight) -> StorageResult<Self::View> {
        // TODO: Unimplemented until of the https://github.com/FuelLabs/fuel-core/issues/451
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1581
        self.clone()
    }
}

impl AtomicView for Database<Relayer> {
    type View = Self;
    type Height = DaBlockHeight;

    fn latest_height(&self) -> Option<Self::Height> {
        *self.height.lock()
    }

    fn view_at(&self, _: &Self::Height) -> StorageResult<Self::View> {
        Ok(self.latest_view())
    }

    fn latest_view(&self) -> Self::View {
        self.clone()
    }
}

impl Modifiable for Database<OnChain> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
                .map(|result| result.map(|(height, _)| height))
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

#[cfg(feature = "relayer")]
impl Modifiable for Database<Relayer> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        commit_changes_with_height_update(self, changes, |iter| {
            iter.iter_all::<fuel_core_relayer::storage::EventsHistory>(Some(
                IterDirection::Reverse,
            ))
            .map(|result| result.map(|(height, _)| height))
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

trait DatabaseHeight: Sized {
    fn as_u64(&self) -> u64;

    fn advance_height(&self) -> Option<Self>;
}

impl DatabaseHeight for BlockHeight {
    fn as_u64(&self) -> u64 {
        let height: u32 = (*self).into();
        height as u64
    }

    fn advance_height(&self) -> Option<Self> {
        self.succ()
    }
}

impl DatabaseHeight for DaBlockHeight {
    fn as_u64(&self) -> u64 {
        self.0
    }

    fn advance_height(&self) -> Option<Self> {
        self.0.checked_add(1).map(Into::into)
    }
}

fn commit_changes_with_height_update<Description>(
    database: &mut Database<Description>,
    changes: Changes,
    heights_lookup: impl Fn(
        &ChangesIterator<Description>,
    ) -> StorageResult<Vec<Description::Height>>,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
    Description::Height: Debug + PartialOrd + DatabaseHeight,
    for<'a> StorageTransaction<&'a &'a mut Database<Description>>:
        StorageMutate<MetadataTable<Description>, Error = StorageError>,
{
    // Gets the all new heights from the `changes`
    let iterator = ChangesIterator::<Description>::new(&changes);
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
    let prev_height = *database.height.lock();

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

            // TODO: After https://github.com/FuelLabs/fuel-core/issues/451
            //  we can replace `next_expected_height > new_height` with `next_expected_height != new_height`.
            if next_expected_height > new_height {
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
            // In production, we shouldn't have cases where we call `commit_chagnes` with intermediate changes.
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
        transaction
            .storage_as_mut::<MetadataTable<Description>>()
            .insert(
                &(),
                &DatabaseMetadata::V1 {
                    version: Description::version(),
                    height: new_height,
                },
            )?;

        transaction.into_changes()
    } else {
        changes
    };

    let mut guard = database.height.lock();
    database
        .data
        .as_ref()
        .commit_changes(new_height, updated_changes)?;

    // Update the block height
    *guard = new_height;

    Ok(())
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
    use fuel_core_storage::{
        tables::FuelBlocks,
        StorageAsMut,
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
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            let advanced_height = 1.into();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&advanced_height, &CompressedBlock::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<OnChain>::default();
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            database
                .storage_as_mut::<Coins>()
                .insert(&UtxoId::default(), &CompressedCoin::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), None);
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
            assert_eq!(database.latest_height().unwrap(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<FuelBlocks>()
                .insert(&next_height, &CompressedBlock::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(next_height));
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
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            let advanced_height = 1.into();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &advanced_height)
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<OffChain>::default();
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            database
                .storage_as_mut::<OwnedMessageIds>()
                .insert(&OwnedMessageKey::default(), &())
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), None);
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
            assert_eq!(database.latest_height().unwrap(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<FuelBlockIdsToHeights>()
                .insert(&Default::default(), &next_height)
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(next_height));
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
        use fuel_core_relayer::storage::{
            DaHeightTable,
            EventsHistory,
        };
        use fuel_core_storage::transactional::WriteTransaction;

        #[test]
        fn column_keys_not_exceed_count_test() {
            column_keys_not_exceed_count::<Relayer>();
        }

        #[test]
        fn database_advances_with_a_new_block() {
            // Given
            let mut database = Database::<Relayer>::default();
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            let advanced_height = 1u64.into();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&advanced_height, &[])
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(advanced_height));
        }

        #[test]
        fn database_not_advances_without_block() {
            // Given
            let mut database = Database::<Relayer>::default();
            assert_eq!(database.latest_height().unwrap(), None);

            // When
            database
                .storage_as_mut::<DaHeightTable>()
                .insert(&(), &DaBlockHeight::default())
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), None);
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
            assert_eq!(database.latest_height().unwrap(), Some(starting_height));

            // When
            let next_height = starting_height.advance_height().unwrap();
            database
                .storage_as_mut::<EventsHistory>()
                .insert(&next_height, &[])
                .unwrap();

            // Then
            assert_eq!(database.latest_height().unwrap(), Some(next_height));
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
            let result = database
                .storage_as_mut::<DaHeightTable>()
                .insert(&(), &DaBlockHeight::default());

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
}
