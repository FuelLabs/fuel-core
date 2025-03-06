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
        HeightType,
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
    structured_storage::TableWithBlueprint,
    transactional::{
        Changes,
        ConflictPolicy,
        ReadTransaction,
        StorageChanges,
        StorageTransaction,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
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
    num::NonZeroU64,
    path::Path,
};

use super::rocks_db::DatabaseConfig;

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
    /// Flag indicating if the database has a history of changes stored in `ModificationsHistoryV1`.
    has_v1_history: core::sync::atomic::AtomicBool,
}

impl<Description> HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(
        db: RocksDb<Historical<Description>>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        let has_v1_history = db
            .iter_all::<ModificationsHistoryV1<Description>>(None)
            .next()
            .is_some();

        Ok(Self {
            state_rewind_policy,
            db,
            has_v1_history: core::sync::atomic::AtomicBool::new(has_v1_history),
        })
    }

    pub fn default_open<P: AsRef<Path>>(
        path: P,
        state_rewind_policy: StateRewindPolicy,
        database_config: DatabaseConfig,
    ) -> DatabaseResult<Self> {
        let db = RocksDb::<Historical<Description>>::default_open(path, database_config)?;
        let has_v1_history = db
            .iter_all::<ModificationsHistoryV1<Description>>(None)
            .next()
            .is_some();
        Ok(Self {
            state_rewind_policy,
            db,
            has_v1_history: core::sync::atomic::AtomicBool::new(has_v1_history),
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
                        if *old_value != **new_value {
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

    /// Create a view at a specific height.
    /// This function relies on the fact that all modifications are sequential
    /// and monotonically grow the height without gaps.
    pub fn create_view_at(
        &self,
        height: &Description::Height,
    ) -> StorageResult<ViewAtHeight<Description>> {
        // Each height stores reverse modification caused by the corresponding
        // block at the same height. Applying reverse changes at height `X`
        // gives us a state at height `X - 1`. If we want a state at height `X`,
        // we need to apply all modifications up to `X + 1`.
        let height_for_the_state = height.as_u64();
        let rollback_height = height_for_the_state.saturating_add(1);
        let has_v1_history = self.has_v1_history();
        let tx = self.db.read_transaction();
        let mut contains = multiversion_contains(&tx, rollback_height, has_v1_history)?;

        // If we don't have any modifications on the height after us,
        // maybe we are at the end of the history.
        // We can check it by checking our height.
        // If modifications exist, then we are the last element of the history
        // that contains an actual state.
        if !contains {
            contains = multiversion_contains(&tx, height_for_the_state, has_v1_history)?;
        }

        if !contains {
            return Err(DatabaseError::NoHistoryForRequestedHeight {
                requested_height: height_for_the_state,
            }
            .into());
        }
        let latest_view = self.db.create_snapshot_generic::<Historical<Description>>();

        Ok(ViewAtHeight::new(rollback_height, latest_view))
    }

    fn store_modifications_history<T>(
        &self,
        storage_transaction: &mut StorageTransaction<T>,
        height: &Description::Height,
    ) -> StorageResult<()>
    where
        T: KeyValueInspect<Column = Column<Description>>,
    {
        if self.state_rewind_policy == StateRewindPolicy::NoRewind {
            return Ok(());
        }
        let height_u64 = height.as_u64();

        let reverse_changes =
            self.reverse_history_changes(storage_transaction.changes())?;

        cleanup_old_changes(
            self,
            &height_u64,
            storage_transaction,
            &self.state_rewind_policy,
        )?;

        let old_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV2<Description>>()
            .replace(&height_u64, &reverse_changes)?;

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

    fn remove_v1_entries(&self) -> StorageResult<()> {
        if !self.has_v1_history() {
            return Ok(())
        }

        self.db
            .clear_table(ModificationsHistoryV1::<Description>::column())?;
        self.has_v1_history
            .store(false, core::sync::atomic::Ordering::Release);

        Ok(())
    }

    #[cfg(test)]
    fn multiversion_changes_heights(
        &self,
        direction: IterDirection,
        has_v1_history: bool,
    ) -> (Option<StorageResult<u64>>, Option<StorageResult<u64>>) {
        let v2_changes = self
            .db
            .iter_all_keys::<ModificationsHistoryV2<Description>>(Some(direction))
            .next();
        let v1_changes = has_v1_history
            .then(|| {
                self.db
                    .iter_all_keys::<ModificationsHistoryV1<Description>>(Some(direction))
                    .next()
            })
            .flatten();

        (v2_changes, v1_changes)
    }

    #[cfg(test)]
    fn rollback_last_block(&self) -> StorageResult<u64> {
        let has_v1_history = self.has_v1_history();

        let (v2_latest_height, v1_latest_height) =
            self.multiversion_changes_heights(IterDirection::Reverse, has_v1_history);

        let latest_height = match (v2_latest_height, v1_latest_height) {
            (None, None) => Err(DatabaseError::ReachedEndOfHistory)?,
            (Some(Ok(v1)), Some(Ok(v2))) => v1.max(v2),
            (_, Some(v1_res)) => v1_res?,
            (Some(v2_res), _) => v2_res?,
        };

        self.rollback_block_to(latest_height)?;

        Ok(latest_height)
    }

    fn rollback_block_to(&self, height_to_rollback: u64) -> StorageResult<()> {
        let mut storage_transaction = self.db.read_transaction();

        let last_changes = multiversion_take(
            &mut storage_transaction,
            height_to_rollback,
            self.has_v1_history(),
        )?
        .ok_or(not_found!(ModificationsHistoryV2<Description>))?;

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
            .commit_changes(&storage_transaction.into_changes().into())?;

        Ok(())
    }

    fn has_v1_history(&self) -> bool {
        use core::sync::atomic::Ordering;

        self.has_v1_history.load(Ordering::Acquire)
    }
}

fn multiversion_contains<Description, T>(
    storage_transaction: &StorageTransaction<T>,
    height: u64,
    has_v1_history: bool,
) -> StorageResult<bool>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
{
    let contains_v2 = storage_transaction
        .storage_as_ref::<ModificationsHistoryV2<Description>>()
        .contains_key(&height)?;

    if !contains_v2 && has_v1_history {
        let contains_v1 = storage_transaction
            .storage_as_ref::<ModificationsHistoryV1<Description>>()
            .contains_key(&height)?;
        Ok(contains_v1)
    } else {
        Ok(contains_v2)
    }
}

// Try to take the value from `ModificationsHistoryV2`, or return value from
// `ModificationsHistoryV1`, if database still has v1 entries.
fn multiversion_take<Description, T>(
    storage_transaction: &mut StorageTransaction<T>,
    height: u64,
    has_v1_history: bool,
) -> StorageResult<Option<Changes>>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
{
    let v2_last_changes = storage_transaction
        .storage_as_mut::<ModificationsHistoryV2<Description>>()
        .take(&height)?;

    if v2_last_changes.is_none() && has_v1_history {
        let v1_last_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV1<Description>>()
            .take(&height)?;
        Ok(v1_last_changes)
    } else {
        Ok(v2_last_changes)
    }
}

fn cleanup_old_changes<Description, T>(
    db: &HistoricalRocksDB<Description>,
    height: &u64,
    storage_transaction: &mut StorageTransaction<T>,
    state_rewind_policy: &StateRewindPolicy,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
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

            let old_changes = storage_transaction
                .storage_as_mut::<ModificationsHistoryV2<Description>>()
                .take(&old_height)?;

            if let Some(old_changes) = old_changes {
                remove_historical_modifications(
                    &old_height,
                    storage_transaction,
                    &old_changes,
                )?;
                // We found end of the V2 history, so we can remove V1 history entirely.
                db.remove_v1_entries()?;
            }
        }
    }
    Ok(())
}

fn remove_historical_modifications<Description, T>(
    old_height: &u64,
    storage_transaction: &mut StorageTransaction<T>,
    reverse_changes: &Changes,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
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
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        self.db
            .read(key, Column::OriginalColumn(column), offset, buf)
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
        mut changes: StorageChanges,
    ) -> StorageResult<()> {
        // When the history need to be process we need to have all the changes in one
        // transaction to be able to write their reverse changes.
        if let Some(height) = height {
            if self.state_rewind_policy != StateRewindPolicy::NoRewind {
                let all_changes = match changes {
                    StorageChanges::Changes(changes) => changes,
                    StorageChanges::ChangesList(list) => {
                        list.into_iter().flatten().collect()
                    }
                };
                let mut storage_transaction = StorageTransaction::transaction(
                    &self.db,
                    ConflictPolicy::Overwrite,
                    all_changes,
                );
                self.store_modifications_history(&mut storage_transaction, &height)?;
                changes = StorageChanges::Changes(storage_transaction.into_changes());
            }
        }

        self.db.commit_changes(&changes)?;

        Ok(())
    }

    fn view_at_height(
        &self,
        height: &Description::Height,
    ) -> StorageResult<KeyValueView<ColumnType<Description>, HeightType<Description>>>
    {
        let view = self.create_view_at(height)?;
        Ok(KeyValueView::from_storage_and_metadata(
            KeyValueViewWrapper::new(view),
            Some(*height),
        ))
    }

    fn latest_view(
        &self,
    ) -> StorageResult<
        IterableKeyValueView<ColumnType<Description>, HeightType<Description>>,
    > {
        let view = self.latest_view();
        Ok(IterableKeyValueView::from_storage_and_metadata(
            IterableKeyValueViewWrapper::new(view),
            None,
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
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // Set the value at height 1 to be 123.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // Set the value at height 2 to be 321.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes().into())
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
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // Set the value at height 1 to be 123.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // Set the value at height 2 to be 321.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes().into())
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
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // When
        let view_at_height_1 =
            historical_rocks_db.create_view_at(&1u32.into()).map(|_| ());

        // Then
        assert_eq!(
            view_at_height_1,
            Err(DatabaseError::NoHistoryForRequestedHeight {
                requested_height: 1,
            }
            .into())
        );
    }

    #[test]
    fn state_rewind_policy__no_rewind__rollback__fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // When
        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Err(DatabaseError::ReachedEndOfHistory.into()));
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__cleanup_in_range_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
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
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // When
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes().into())
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
            }
            .into())
        );
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__rollback_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
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
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
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
    fn state_rewind_policy__rewind_range_1__rollback_uses_v2() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        // When
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .collect::<Vec<_>>();

        // Then
        assert_eq!(v2_entries.len(), 1);
        assert_eq!(v1_entries.len(), 0);
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__rollback_during_migration_works() {
        // Given
        let temp_dir = tempfile::tempdir().unwrap();
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open(
            &temp_dir,
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        // When
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
            .unwrap();

        // Migrate the changes from V2 to V1.

        let mut migration_transaction = StorageTransaction::transaction(
            &historical_rocks_db.db,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        let v2_changes = migration_transaction
            .storage_as_mut::<ModificationsHistoryV2<OnChain>>()
            .take(&1u64)
            .unwrap()
            .unwrap();
        migration_transaction
            .storage_as_mut::<ModificationsHistoryV1<OnChain>>()
            .insert(&1u64, &v2_changes)
            .unwrap();

        historical_rocks_db
            .db
            .commit_changes(&migration_transaction.into_changes().into())
            .unwrap();

        // Check that the history has indeed been written to V1
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .collect::<Vec<_>>();

        assert_eq!(v2_entries.len(), 0);
        assert_eq!(v1_entries.len(), 1);

        drop(historical_rocks_db);

        // Open the database again with fetched V1 entries status.
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open(
            &temp_dir,
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();
        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Ok(1));
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .collect::<Vec<_>>();
        assert_eq!(v2_entries.len(), 0);
        assert_eq!(v1_entries.len(), 0);
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__migration_removes_v1() {
        let temp_dir = tempfile::tempdir().unwrap();
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open(
            &temp_dir,
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        const BLOCKS: u8 = 10;

        // Given
        // Create 10 blocks with some changes.
        // And migrate all changes to V1 history.
        for i in 1..=BLOCKS {
            let height = i as u32;
            let height_64 = i as u64;
            let mut transaction = historical_rocks_db.read_transaction();
            let key = ContractsAssetKey::new(&[i; 32].into(), &[213; 32].into());
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key, &123)
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(height.into()), transaction.into_changes().into())
                .unwrap();

            // Migrate the changes from V2 to V1.
            let mut migration_transaction = StorageTransaction::transaction(
                &historical_rocks_db.db,
                ConflictPolicy::Overwrite,
                Changes::default(),
            );

            let v2_changes = migration_transaction
                .storage_as_mut::<ModificationsHistoryV2<OnChain>>()
                .take(&height_64)
                .unwrap()
                .unwrap();
            migration_transaction
                .storage_as_mut::<ModificationsHistoryV1<OnChain>>()
                .insert(&height_64, &v2_changes)
                .unwrap();

            historical_rocks_db
                .db
                .commit_changes(&migration_transaction.into_changes().into())
                .unwrap();

            // Check that the history has indeed been written to V1
            let v2_entries = historical_rocks_db
                .db
                .iter_all::<ModificationsHistoryV2<OnChain>>(None)
                .collect::<Vec<_>>();
            let v1_entries = historical_rocks_db
                .db
                .iter_all::<ModificationsHistoryV1<OnChain>>(None)
                .collect::<Vec<_>>();

            assert_eq!(v2_entries.len(), 0);
            assert_eq!(v1_entries.len(), i as usize);
        }

        drop(historical_rocks_db);

        // When
        // Open the database again, but with the rewind range of 1.
        // Committing 2 new blocks, should add new entries to V2 history.
        // And because of the rewind range of 1, the V1 history should be removed.
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open(
            &temp_dir,
            DatabaseConfig::config_for_tests(),
        )
        .unwrap();
        let historical_rocks_db = HistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();
        historical_rocks_db
            .commit_changes(Some((BLOCKS as u32 + 1).into()), StorageChanges::default())
            .unwrap();
        historical_rocks_db
            .commit_changes(Some((BLOCKS as u32 + 2).into()), StorageChanges::default())
            .unwrap();

        // Then
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .collect::<Vec<_>>();
        assert_eq!(v2_entries.len(), 1);
        assert_eq!(v1_entries.len(), 0);
    }

    #[test]
    fn rollback_last_block_works_with_v2() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();

        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // When
        // Commit 1000 blocks
        for i in 1..=1000u32 {
            let mut transaction = historical_rocks_db.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key(), &(123 + i as u64))
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(i.into()), transaction.into_changes().into())
                .unwrap();
        }
        // We can now rollback the last block 1000 times.
        let results: Vec<Result<u64, _>> = (0..1000u32)
            .map(|_| historical_rocks_db.rollback_last_block())
            .collect();

        // Then
        // If the rollback fails at some point, then we have unintentionally rollbacked to
        // a block that was not the last.
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result, &Ok(1000 - i as u64));
        }
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__second_rollback_fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
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
            .commit_changes(Some(1u32.into()), transaction.into_changes().into())
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

        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp().unwrap();
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
                .commit_changes(Some(height.into()), transaction.into_changes().into())
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
