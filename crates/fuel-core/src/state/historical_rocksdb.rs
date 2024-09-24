use crate::{
    database::{
        database_description::{
            DatabaseDescription,
            DatabaseHeight,
        },
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::{
        historical_rocksdb::{
            description::{
                historical_duplicate_column_id,
                Column,
                Historical,
            },
            view_at_height::ViewAtHeight,
        },
        iterable_key_value_view::IterableKeyValueViewWrapper,
        key_value_view::KeyValueViewWrapper,
        rocks_db::RocksDb,
        ColumnType,
        IterableKeyValueView,
        KeyValueView,
        TransactableStorage,
    },
};
use fuel_core_storage::{
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
        WriteOperation,
    },
    not_found,
    transactional::{
        Changes,
        ConflictPolicy,
        ReadTransaction,
        StorageTransaction,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsMut,
};
use itertools::Itertools;
use modifications_history::{
    ModificationsHistoryV1,
    ModificationsHistoryV2,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    borrow::Cow,
    num::NonZeroU64,
    path::Path,
};

pub mod description;
pub mod modifications_history;
pub mod view_at_height;

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
/// Defined policies for the state rewind behaviour of the database.
pub enum StateRewindPolicy {
    #[default]
    /// The checkpoint will be created only for the latest height.
    NoRewind,
    /// The checkpoint will be created for each height.
    RewindFullRange,
    /// The checkpoint will be created for each height
    /// in the range `[latest_height-size..latest_height]`.
    RewindRange { size: NonZeroU64 },
}

/// Implementation of a database
#[derive(Debug)]
pub struct HistoricalRocksDB<Description> {
    /// The [`StateRewindPolicy`] used by the historical rocksdb
    state_rewind_policy: StateRewindPolicy,
    /// The Description of the database.
    db: RocksDb<Historical<Description>>,
    /// Indicates whether the [`Column::ModificationsHistory`]
    /// is being migrated to [`Column::ModificationsHistoryV2`]
    migration_in_progress: bool,
}

impl<Description> HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(
        db: RocksDb<Historical<Description>>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        Ok(Self {
            state_rewind_policy,
            db,
            migration_in_progress: true,
        })
    }

    pub fn default_open<P: AsRef<Path>>(
        path: P,
        capacity: Option<usize>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        let db = RocksDb::<Historical<Description>>::default_open(path, capacity)?;
        Ok(Self {
            state_rewind_policy,
            db,
            migration_in_progress: true,
        })
    }

    fn reverse_history_changes(&self, changes: &Changes) -> StorageResult<Changes> {
        let mut reverse_changes = Changes::default();

        for (column, column_changes) in changes {
            let results = self
                .db
                .multi_get(*column, column_changes.iter().map(|(k, _)| k))?;

            let entry = reverse_changes
                .entry(*column)
                .or_insert_with(Default::default);

            for (was, (key, became)) in results.into_iter().zip(column_changes.iter()) {
                match (was, became) {
                    (None, WriteOperation::Remove) => {
                        // Do nothing since it was not existing, and it was removed.
                    }
                    (None, WriteOperation::Insert(_)) => {
                        entry.insert(key.clone(), WriteOperation::Remove);
                    }
                    (Some(old_value), WriteOperation::Remove) => {
                        entry.insert(
                            key.clone(),
                            WriteOperation::Insert(old_value.into()),
                        );
                    }
                    (Some(old_value), WriteOperation::Insert(new_value)) => {
                        if old_value.as_slice() != new_value.as_slice() {
                            entry.insert(
                                key.clone(),
                                WriteOperation::Insert(old_value.into()),
                            );
                        }
                    }
                }
            }
        }
        Ok(reverse_changes)
    }

    pub fn latest_view(&self) -> RocksDb<Description> {
        self.db.create_snapshot_generic()
    }

    pub fn create_view_at(
        &self,
        height: &Description::Height,
    ) -> StorageResult<ViewAtHeight<Description>> {
        let latest_view = self.db.create_snapshot_generic::<Historical<Description>>();

        // Each height stores reverse modification caused by the corresponding
        // block at the same height. Applying reverse changes at height `X`
        // gives us a state at height `X - 1`. If we want a state at height `X`,
        // we need to apply all modifications up to `X + 1`.
        let rollback_height = height.as_u64().saturating_add(1);

        // TODO: May fail incorrectly because of https://github.com/FuelLabs/fuel-core/issues/2095
        let Some(oldest_height) = self.oldest_changes_height()? else {
            return Err(DatabaseError::NoHistoryIsAvailable.into());
        };

        if rollback_height < oldest_height {
            return Err(DatabaseError::NoHistoryForRequestedHeight {
                requested_height: height.as_u64(),
                oldest_available_height: oldest_height.saturating_sub(1),
            }
            .into());
        }

        Ok(ViewAtHeight::new(rollback_height, latest_view))
    }

    fn store_modifications_history(
        &self,
        storage_transaction: &mut StorageTransaction<&RocksDb<Historical<Description>>>,
        height: &Description::Height,
    ) -> StorageResult<()> {
        if self.state_rewind_policy == StateRewindPolicy::NoRewind {
            return Ok(());
        }
        let height_u64 = height.as_u64();

        let reverse_changes =
            self.reverse_history_changes(storage_transaction.changes())?;

        cleanup_old_changes(
            &height_u64,
            self.migration_in_progress,
            storage_transaction,
            &self.state_rewind_policy,
        )?;

        // We write directly to `ModificationsHistoryV2`.
        // If the migration is in progress, we fallback to reading from
        // `ModificationsHistoryV1` when no old_changes for `ModificationsHistoryV2` are found.
        let v2_old_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV2<Description>>()
            .replace(&height_u64, &reverse_changes)?;

        let old_changes = match v2_old_changes {
            None if self.migration_in_progress => storage_transaction
                .storage_as_mut::<ModificationsHistoryV1<Description>>()
                .get(&height_u64)?
                .map(Cow::into_owned),
            _ => v2_old_changes,
        };

        if let Some(old_changes) = old_changes {
            tracing::warn!(
                "Historical database committed twice the same height: {:?}",
                height
            );
            remove_historical_modifications(
                &height_u64,
                storage_transaction,
                &old_changes,
            )?;
        }

        let historical_changes = reverse_changes
            .into_iter()
            .map(|(column, reverse_column_changes)| {
                let historical_column_changes = reverse_column_changes
                    .into_iter()
                    .map(|(key, reverse_operation)| {
                        let height_key = height_key(&key, &height_u64).into();
                        // We want to store the operation that we want
                        // to apply during rollback as a value.
                        let operation =
                            WriteOperation::Insert(serialize(&reverse_operation)?);
                        Ok::<_, StorageError>((height_key, operation))
                    })
                    .try_collect()?;

                let historical_duplicate_column = historical_duplicate_column_id(column);
                Ok::<_, StorageError>((
                    historical_duplicate_column,
                    historical_column_changes,
                ))
            })
            .try_collect()?;

        // Combine removed old changes, all modifications for
        // the current height and historical changes.
        StorageTransaction::transaction(
            storage_transaction,
            ConflictPolicy::Overwrite,
            historical_changes,
        )
        .commit()?;
        Ok(())
    }

    fn oldest_changes_height(&self) -> StorageResult<Option<u64>> {
        let v2_oldest_height = self
            .db
            .iter_all_keys::<ModificationsHistoryV2<Description>>(Some(
                IterDirection::Forward,
            ))
            .next()
            .transpose()?;

        let v1_oldest_height = if self.migration_in_progress {
            self.db
                .iter_all_keys::<ModificationsHistoryV1<Description>>(Some(
                    IterDirection::Forward,
                ))
                .next()
                .transpose()?
        } else {
            None
        };

        let oldest_height = match (v1_oldest_height, v2_oldest_height) {
            (None, v2) => v2,
            (v1, None) => v1,
            (Some(v1), Some(v2)) => Some(v1.min(v2)),
        };
        Ok(oldest_height)
    }

    #[cfg(test)]
    // TODO: This method doesn't work properly because of
    //  https://github.com/FuelLabs/fuel-core/issues/2095
    fn rollback_last_block(&self) -> StorageResult<u64> {
        let v2_latest_height = self
            .db
            .iter_all_keys::<ModificationsHistoryV2<Description>>(Some(
                IterDirection::Reverse,
            ))
            .next();

        // TODO: Maybe I could avoid iterating over V1, since it is broken anyway?
        let v1_latest_height = if self.migration_in_progress {
            self.db
                .iter_all_keys::<ModificationsHistoryV1<Description>>(Some(
                    IterDirection::Reverse,
                ))
                .next()
        } else {
            None
        };

        let latest_height = match (v2_latest_height, v1_latest_height) {
            (None, None) => Err(DatabaseError::ReachedEndOfHistory)?,
            (None, Some(v1_res)) => v1_res?,
            (Some(v2_res), None) => v2_res?,
            (Some(Err(_)), Some(v1_res)) => v1_res?,
            (Some(v2_res), Some(Err(_))) => v2_res?,
            (Some(Ok(v1)), Some(Ok(v2))) => v1.min(v2),
        };

        self.rollback_block_to(latest_height)?;

        Ok(latest_height)
    }

    fn rollback_block_to(&self, height_to_rollback: u64) -> StorageResult<()> {
        let mut storage_transaction = self.db.read_transaction();

        let v2_last_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV2<Description>>()
            .take(&height_to_rollback)?;

        let last_changes = match v2_last_changes {
            None if self.migration_in_progress => storage_transaction
                .storage_as_mut::<ModificationsHistoryV1<Description>>()
                .get(&height_to_rollback)?
                .map(Cow::into_owned)
                .ok_or(not_found!(ModificationsHistoryV1<Description>))?, /* Is this the right error? */
            None => return Err(not_found!(ModificationsHistoryV2<Description>)),
            Some(v2_last_changes) => v2_last_changes,
        };

        remove_historical_modifications(
            &height_to_rollback,
            &mut storage_transaction,
            &last_changes,
        )?;

        StorageTransaction::transaction(
            &mut storage_transaction,
            ConflictPolicy::Overwrite,
            last_changes,
        )
        .commit()?;

        self.db
            .commit_changes(&storage_transaction.into_changes())?;

        Ok(())
    }
}

fn cleanup_old_changes<Description>(
    height: &u64,
    migration_in_progress: bool,
    storage_transaction: &mut StorageTransaction<&RocksDb<Historical<Description>>>,
    state_rewind_policy: &StateRewindPolicy,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
{
    match state_rewind_policy {
        StateRewindPolicy::NoRewind => {
            // Do nothing since we do not store any history.
        }
        StateRewindPolicy::RewindFullRange => {
            // Do nothing since we store all history.
        }
        StateRewindPolicy::RewindRange { size } => {
            let old_height = height.saturating_sub(size.get());

            let v2_old_changes = storage_transaction
                .storage_as_mut::<ModificationsHistoryV2<Description>>()
                .take(&old_height)?;

            let old_changes = match v2_old_changes {
                None if migration_in_progress => storage_transaction
                    .storage_as_mut::<ModificationsHistoryV1<Description>>()
                    .get(&old_height)?
                    .map(Cow::into_owned),
                _ => v2_old_changes,
            };

            if let Some(old_changes) = old_changes {
                remove_historical_modifications(
                    &old_height,
                    storage_transaction,
                    &old_changes,
                )?;
            }
        }
    }
    Ok(())
}

fn remove_historical_modifications<Description>(
    old_height: &u64,
    storage_transaction: &mut StorageTransaction<&RocksDb<Historical<Description>>>,
    reverse_changes: &Changes,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
{
    let changes = reverse_changes
        .iter()
        .map(|(column, column_changes)| {
            let historical_column_changes = column_changes
                .keys()
                .map(|key| {
                    let height_key = height_key(key, old_height).into();
                    let operation = WriteOperation::Remove;
                    (height_key, operation)
                })
                .collect();
            let historical_duplicate_column = historical_duplicate_column_id(*column);
            (historical_duplicate_column, historical_column_changes)
        })
        .collect();

    StorageTransaction::transaction(
        storage_transaction,
        ConflictPolicy::Overwrite,
        changes,
    )
    .commit()?;

    Ok(())
}

impl<Description> KeyValueInspect for HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.db.exists(key, Column::OriginalColumn(column))
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.db.size_of_value(key, Column::OriginalColumn(column))
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.db.get(key, Column::OriginalColumn(column))
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.db.read(key, Column::OriginalColumn(column), buf)
    }
}

impl<Description> IterableStore for HistoricalRocksDB<Description>
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
        self.db
            .iter_store(Column::OriginalColumn(column), prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        self.db
            .iter_store_keys(Column::OriginalColumn(column), prefix, start, direction)
    }
}

impl<Description> TransactableStorage<Description::Height>
    for HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    fn commit_changes(
        &self,
        height: Option<Description::Height>,
        changes: Changes,
    ) -> StorageResult<()> {
        let mut storage_transaction =
            StorageTransaction::transaction(&self.db, ConflictPolicy::Overwrite, changes);
        if let Some(height) = height {
            self.store_modifications_history(&mut storage_transaction, &height)?;
        }

        self.db
            .commit_changes(&storage_transaction.into_changes())?;
        Ok(())
    }

    fn view_at_height(
        &self,
        height: &Description::Height,
    ) -> StorageResult<KeyValueView<ColumnType<Description>>> {
        let view = self.create_view_at(height)?;
        Ok(KeyValueView::from_storage(KeyValueViewWrapper::new(view)))
    }

    fn latest_view(
        &self,
    ) -> StorageResult<IterableKeyValueView<ColumnType<Description>>> {
        let view = self.latest_view();
        Ok(IterableKeyValueView::from_storage(
            IterableKeyValueViewWrapper::new(view),
        ))
    }

    fn rollback_block_to(&self, height: &Description::Height) -> StorageResult<()> {
        self.rollback_block_to(height.as_u64())
    }
}

pub fn height_key(key: &[u8], height: &u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(key.len().saturating_add(8));
    let height_bytes = height.to_be_bytes();
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(&height_bytes);
    bytes
}

pub fn serialize<T>(t: &T) -> StorageResult<Value>
where
    T: Serialize + ?Sized,
{
    Ok(postcard::to_allocvec(&t)
        .map_err(|err| StorageError::Codec(err.into()))?
        .into())
}

pub fn deserialize<'a, T>(bytes: &'a [u8]) -> StorageResult<T>
where
    T: Deserialize<'a>,
{
    postcard::from_bytes(bytes).map_err(|err| StorageError::Codec(err.into()))
}

#[cfg(test)]
#[allow(non_snake_case)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::{
        tables::ContractsAssets,
        transactional::{
            IntoTransaction,
            ReadTransaction,
        },
        ContractsAssetKey,
        StorageAsMut,
        StorageAsRef,
    };

    #[test]
    fn test_height_key() {
        let key = b"key";
        let height = 42;
        let expected = b"key\x00\x00\x00\x00\x00\x00\x00\x2a";
        assert_eq!(height_key(key, &height), expected);
    }

    fn key() -> ContractsAssetKey {
        ContractsAssetKey::new(&[123; 32].into(), &[213; 32].into())
    }

    #[test]
    fn historical_rocksdb_read_original_database_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // Set the value at height 1 to be 123.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();

        // Set the value at height 2 to be 321.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let read_view = historical_rocks_db.read_transaction();
        let latest_balance = read_view
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(latest_balance, 321);
    }

    #[test]
    fn historical_rocksdb_read_latest_view_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // Set the value at height 1 to be 123.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();

        // Set the value at height 2 to be 321.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let latest_view = historical_rocks_db.latest_view().into_transaction();
        let latest_balance = latest_view
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(latest_balance, 321);
    }

    #[test]
    fn state_rewind_policy__no_rewind__create_view_at__fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let view_at_height_1 =
            historical_rocks_db.create_view_at(&1u32.into()).map(|_| ());

        // Then
        assert_eq!(
            view_at_height_1,
            Err(DatabaseError::NoHistoryIsAvailable.into())
        );
    }

    #[test]
    fn state_rewind_policy__no_rewind__rollback__fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Err(DatabaseError::ReachedEndOfHistory.into()));
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__cleanup_in_range_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes())
            .unwrap();

        // Then
        let view_at_height_1 =
            historical_rocks_db.create_view_at(&1u32.into()).map(|_| ());
        let view_at_height_0 =
            historical_rocks_db.create_view_at(&0u32.into()).map(|_| ());
        assert_eq!(view_at_height_1, Ok(()));
        assert_eq!(
            view_at_height_0,
            Err(DatabaseError::NoHistoryForRequestedHeight {
                requested_height: 0,
                oldest_available_height: 1,
            }
            .into())
        );
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__rollback_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();
        let entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);

        // When
        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Ok(1));
        let entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__second_rollback_fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();
        historical_rocks_db.rollback_last_block().unwrap();

        // When
        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Err(DatabaseError::ReachedEndOfHistory.into()));
    }

    #[test]
    fn state_rewind_policy__rewind_range_10__rollbacks_work() {
        const ITERATIONS: usize = 100;

        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(ITERATIONS as u64).unwrap(),
            },
        )
        .unwrap();

        fn key(height: u32) -> ContractsAssetKey {
            ContractsAssetKey::new(&[height as u8; 32].into(), &[213; 32].into())
        }

        for height in 1..=ITERATIONS {
            let height = height as u32;
            let key = key(height);

            let mut transaction = historical_rocks_db.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key, &123)
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(height.into()), transaction.into_changes())
                .unwrap();
        }

        for height in (1..=ITERATIONS).rev() {
            // Given
            let entries = historical_rocks_db
                .db
                .iter_all::<ModificationsHistoryV2<OnChain>>(None)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), height);

            // When
            let result = historical_rocks_db.rollback_last_block();

            // Then
            assert_eq!(result, Ok(height as u64));
            let entries = historical_rocks_db
                .db
                .iter_all::<ModificationsHistoryV2<OnChain>>(None)
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), height - 1);
        }
    }
}
