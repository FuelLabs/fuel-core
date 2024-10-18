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
use parking_lot::{
    Mutex as ParkingMutex,
    MutexGuard as ParkingMutexGuard,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    num::NonZeroU64,
    path::Path,
    sync::Arc,
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

/// A structure representing a historical RocksDB.
/// The HistoricalRocksDB augments the RocksDB with
/// the ability to store the modification history of changes
/// made when creating a new view. This is used to rollback the database to
/// a previous state.
/// Each HistoricalRocksDB specifies a [StateRewindPolicy] that determines
/// the retention policy of the modification history.
#[derive(Debug)]
pub struct HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    inner: Arc<InnerHistoricalRocksDB<Description>>,
}

impl<Description> HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    /// Creates a HistoricalRocksDB instance from an existing RocksDB instance.
    /// Upon invoking this function, a separate thread will be spawned to migrate
    /// the modification history from V1 to V2.
    pub fn new(
        db: RocksDb<Historical<Description>>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        let inner = Arc::new(InnerHistoricalRocksDB::new(db, state_rewind_policy)?);
        Self::migrate_modifications_history(inner.clone());
        Ok(Self { inner })
    }

    /// Opens a HistoricalRocksDB instance from the given path.
    /// Upon invoking this function, a separate thread will be spawned to migrate
    /// the modification history from V1 to V2.
    pub fn default_open<P: AsRef<Path>>(
        path: P,
        capacity: Option<usize>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        let inner = Arc::new(InnerHistoricalRocksDB::default_open(
            path,
            capacity,
            state_rewind_policy,
        )?);
        Self::migrate_modifications_history(inner.clone());
        Ok(Self { inner })
    }

    fn migrate_modifications_history(
        historical_rocksdb: Arc<InnerHistoricalRocksDB<Description>>,
    ) {
        historical_rocksdb.is_migration_in_progress().then(|| {
            tokio::task::spawn_blocking(move || {
                let entries = historical_rocksdb.v1_entries();
                for entry in entries {
                    match entry {
                        Err(e) => {
                        tracing::error!("Cannot read from database while migrating modification history: {e:?}");
                        break;
                    }
                        Ok((height, _change)) => {
                            historical_rocksdb
                                .migrate_modifications_history_at_height(height)
                                .expect("Accumulating migration changes cannot fail");
                        }
                    }
                }
            });
            Some(())
        });
    }

    /// Returns the latest view of the database.
    pub fn latest_view(&self) -> RocksDb<Description> {
        self.inner.latest_view()
    }

    /// Creates a view at the given height.
    pub fn create_view_at(
        &self,
        height: &Description::Height,
    ) -> StorageResult<ViewAtHeight<Description>> {
        self.inner.create_view_at(height)
    }
}

impl<Description> TryFrom<InnerHistoricalRocksDB<Description>>
    for HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    type Error = DatabaseError;

    fn try_from(inner: InnerHistoricalRocksDB<Description>) -> DatabaseResult<Self> {
        let InnerHistoricalRocksDB {
            db,
            state_rewind_policy,
            ..
        } = inner;
        Self::new(db, state_rewind_policy)
    }
}

impl<Description> KeyValueInspect for HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.inner.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.inner.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.inner.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.inner.read(key, column, buf)
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
        self.inner.iter_store(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        self.inner.iter_store_keys(column, prefix, start, direction)
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
        self.inner.commit_changes(height, changes)
    }

    fn view_at_height(
        &self,
        height: &Description::Height,
    ) -> StorageResult<KeyValueView<ColumnType<Description>>> {
        self.inner.view_at_height(height)
    }

    fn latest_view(
        &self,
    ) -> StorageResult<IterableKeyValueView<ColumnType<Description>>> {
        <InnerHistoricalRocksDB<Description> as TransactableStorage<
            Description::Height,
        >>::latest_view(&*self.inner)
    }

    fn rollback_block_to(&self, height: &Description::Height) -> StorageResult<()> {
        <InnerHistoricalRocksDB<Description> as TransactableStorage<
            Description::Height,
        >>::rollback_block_to(&*self.inner, height)
    }
}

/// Implementation of a database
#[derive(Debug)]
struct InnerHistoricalRocksDB<Description> {
    /// The [`StateRewindPolicy`] used by the historical rocksdb
    state_rewind_policy: StateRewindPolicy,
    /// The Description of the database.
    db: RocksDb<Historical<Description>>,
    /// Changes that have been committed in memory, but not to rocksdb
    migration_changes: ParkingMutex<Changes>,
}

impl<Description> InnerHistoricalRocksDB<Description>
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
            migration_changes: ParkingMutex::new(Changes::default()),
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
            migration_changes: ParkingMutex::new(Changes::default()),
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

    fn store_modifications_history<T>(
        &self,
        storage_transaction: &mut StorageTransaction<T>,
        height: &Description::Height,
    ) -> StorageResult<()>
    where
        T: KeyValueInspect<Column = Column<Description>>,
    {
        let modifications_history_migration_in_progress = self.is_migration_in_progress();

        if self.state_rewind_policy == StateRewindPolicy::NoRewind {
            return Ok(());
        }
        let height_u64 = height.as_u64();

        let reverse_changes =
            self.reverse_history_changes(storage_transaction.changes())?;

        cleanup_old_changes(
            &height_u64,
            storage_transaction,
            &self.state_rewind_policy,
            modifications_history_migration_in_progress,
        )?;

        // We write directly to `ModificationsHistoryV2`.
        // If the migration is in progress, we fallback to taking from
        // `ModificationsHistoryV1` when no old_changes for `ModificationsHistoryV2` are found.
        let old_changes = multiversion_replace(
            storage_transaction,
            height_u64,
            &reverse_changes,
            modifications_history_migration_in_progress,
        )?;

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

    fn multiversion_changes_heights(
        &self,
        direction: IterDirection,
        check_v1: bool,
    ) -> (Option<StorageResult<u64>>, Option<StorageResult<u64>>) {
        let v2_changes = self
            .db
            .iter_all_keys::<ModificationsHistoryV2<Description>>(Some(direction))
            .next();
        let v1_changes = check_v1
            .then(|| {
                self.db
                    .iter_all_keys::<ModificationsHistoryV1<Description>>(Some(direction))
                    .next()
            })
            .flatten();

        (v2_changes, v1_changes)
    }

    fn oldest_changes_height(&self) -> StorageResult<Option<u64>> {
        let modifications_history_migration_in_progress = self.is_migration_in_progress();

        let (v2_oldest_height, v1_oldest_height) = self.multiversion_changes_heights(
            IterDirection::Forward,
            modifications_history_migration_in_progress,
        );

        let v2_oldest_height = v2_oldest_height.transpose()?;
        let v1_oldest_height = v1_oldest_height.transpose()?;

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
        let modifications_history_migration_in_progress = self.is_migration_in_progress();

        let (v2_latest_height, v1_latest_height) = self.multiversion_changes_heights(
            IterDirection::Reverse,
            modifications_history_migration_in_progress,
        );

        let latest_height = match (v2_latest_height, v1_latest_height) {
            (None, None) => Err(DatabaseError::ReachedEndOfHistory)?,
            (Some(Ok(v1)), Some(Ok(v2))) => v1.min(v2),
            (_, Some(v1_res)) => v1_res?,
            (Some(v2_res), _) => v2_res?,
        };

        self.rollback_block_to(latest_height)?;

        Ok(latest_height)
    }

    fn take_migration_changes(&self) -> (Changes, Option<ParkingMutexGuard<Changes>>) {
        if self.is_migration_in_progress() {
            let mut lock_guard = self.migration_changes.lock();
            (std::mem::take(&mut *lock_guard), Some(lock_guard))
        } else {
            (Changes::default(), None)
        }
    }

    fn add_migration_changes(
        &self,
        lock_guard: Option<ParkingMutexGuard<Changes>>,
        changes: Changes,
    ) -> StorageResult<()> {
        match lock_guard {
            // The only case when we did not acquire a lock_guard is because the migration is not in progress.
            // We do not need to do anything in this case.
            None => Ok(()),
            Some(mut lock_guard) => {
                let current_changes = std::mem::take(&mut *lock_guard);
                let mut current_changes_transaction = StorageTransaction::transaction(
                    &self.db,
                    ConflictPolicy::Overwrite,
                    current_changes,
                );

                StorageTransaction::transaction(
                    &mut current_changes_transaction,
                    ConflictPolicy::Overwrite,
                    changes,
                )
                .commit()?;

                *lock_guard = current_changes_transaction.into_changes();

                Ok(())
            }
        }
    }

    fn rollback_block_to(&self, height_to_rollback: u64) -> StorageResult<()> {
        let (cumulative_changes, maybe_cumulative_changes_guard) =
            self.take_migration_changes();

        // Will clone an empty set of changes if the migration is not in progress, which should
        // not impact performance. However, when a migration is in progress, this operation could
        // be expensive if many changes have been accumulated.
        let mut migration_transaction = StorageTransaction::transaction(
            &self.db,
            ConflictPolicy::Overwrite,
            cumulative_changes.clone(),
        );

        let mut storage_transaction = StorageTransaction::transaction(
            &mut migration_transaction,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        let last_changes = multiversion_take(
            &mut storage_transaction,
            height_to_rollback,
            self.is_migration_in_progress(),
        )?
        .ok_or(not_found!(ModificationsHistoryV1<Description>))?;

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

        storage_transaction.commit()?;

        match self
            .db
            .commit_changes(&migration_transaction.into_changes())
        {
            Ok(()) => Ok(()),
            Err(err) => {
                tracing::error!(
                    "Could not rollback historical rocksDB to height {}: {err:?}",
                    err
                );
                // If the migration is not in progress, this function will effectively be a no-op
                self.add_migration_changes(
                    maybe_cumulative_changes_guard,
                    cumulative_changes,
                )?;
                Err(err)
            }
        }
    }

    /// Migrates a ModificationHistory key-value pair from V1 to V2.
    /// The migration is lazy, in that the changes necessary to perform
    /// the migration from ModificationHistoryV1 to ModificationHistoryV2
    /// are simply recorded, and will be flushed to the database when it
    /// commits or rollbacks to a new height.
    fn migrate_modifications_history_at_height(&self, height: u64) -> StorageResult<()> {
        // We need to keep the lock to the migration changes until the end of the function.
        // This is to avoid a scenario where the migration changes overwrites changes to the modification history
        // coming from other transactions.
        // If we were to release the lock as soon as we take the cumulative changes, and then acquiring it again when
        // writing the updatet set of changes, then we could have the following schedule of events.
        // - A modification transaction M1 migrates height 42 from V1 to V2.
        // - A second modification transaction M2 takes the cumulative changes from the lock, and adds the changes for another height.
        // - A transaction T1 rollbacks to height 42, writes the changes to the modification history, and commits to rocksDB.
        // - Transaction M2 acquires the lock to write the updated cumulative changes, which include the
        //   migration of the history for height 42 from V1 to V2.
        // - The next time a transaction commits to RocksDB, the cumulative changes from the migration transactions will be written to RocksDB,
        //   which will cause the update from transaction T1 to be lost.

        // This function is called only when a migration is in progress, hence the lock_guard is guaranteed to be Some.
        let (cumulative_changes, lock_guard) = self.take_migration_changes();
        let mut cumulative_transaction = StorageTransaction::transaction(
            &self.db,
            ConflictPolicy::Overwrite,
            cumulative_changes,
        );

        // TODO: I could add a second migration transaction and commit the second transaction into the first
        let mut migration_transaction = StorageTransaction::transaction(
            &mut cumulative_transaction,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        let v1_changes = migration_transaction
            .storage_as_mut::<ModificationsHistoryV1<Description>>()
            .take(&height)?;
        if let Some(v1_changes) = v1_changes {
            migration_transaction
                .storage_as_mut::<ModificationsHistoryV2<Description>>()
                .insert(&height, &v1_changes)?; // TODO: If this is an error we want to at least write back the old changes
        };

        // Add the changes into the cumulative changes
        migration_transaction.commit()?;

        self.add_migration_changes(lock_guard, cumulative_transaction.into_changes())?;

        Ok(())
    }

    fn v1_entries(&self) -> BoxedIter<StorageResult<(u64, Changes)>> {
        self.db
            .iter_all::<ModificationsHistoryV1<Description>>(None)
    }

    fn is_migration_in_progress(&self) -> bool {
        self.v1_entries().next().is_some()
    }

    #[cfg(test)]
    fn commit_migration_changes(&self) -> StorageResult<()> {
        self.db
            .commit_changes(&std::mem::take(&mut *self.migration_changes.lock()))
    }
}

// Try to take the value from `ModificationsHistoryV2` first.
// If the migration is still in progress, remove the value from
// `ModificationsHistoryV1` and return it if no value for `ModificationsHistoryV2`
// was found. This is necessary to avoid scenarios where it is possible to
// roll back twice to the same block height
fn multiversion_take<Description, T>(
    storage_transaction: &mut StorageTransaction<T>,
    height: u64,
    modifications_history_migration_in_progress: bool,
) -> StorageResult<Option<Changes>>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
{
    // This will cause the V2 key to be removed in case the storage transaction snapshot
    // a conflicting transaction writes a value for it, but that update is not reflected
    // in the storage transaction snapshot.
    let v2_last_changes = storage_transaction
        .storage_as_mut::<ModificationsHistoryV2<Description>>()
        .take(&height)?;

    if modifications_history_migration_in_progress {
        let v1_last_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV1<Description>>()
            .take(&height)?;
        Ok(v2_last_changes.or(v1_last_changes))
    } else {
        Ok(v2_last_changes)
    }
}

fn multiversion_replace<Description, T>(
    storage_transaction: &mut StorageTransaction<T>,
    height: u64,
    changes: &Changes,
    modifications_history_migration_in_progress: bool,
) -> StorageResult<Option<Changes>>
where
    Description: DatabaseDescription,
    T: KeyValueInspect<Column = Column<Description>>,
{
    let v2_last_changes = storage_transaction
        .storage_as_mut::<ModificationsHistoryV2<Description>>()
        .replace(&height, changes)?;

    if modifications_history_migration_in_progress {
        let v1_last_changes = storage_transaction
            .storage_as_mut::<ModificationsHistoryV1<Description>>()
            .take(&height)?;
        Ok(v2_last_changes.or(v1_last_changes))
    } else {
        Ok(v2_last_changes)
    }
}

fn cleanup_old_changes<Description, T>(
    height: &u64,
    storage_transaction: &mut StorageTransaction<T>,
    state_rewind_policy: &StateRewindPolicy,
    modifications_history_migration_in_progress: bool,
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

            let old_changes = multiversion_take(
                storage_transaction,
                old_height,
                modifications_history_migration_in_progress,
            )?;

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

impl<Description> KeyValueInspect for InnerHistoricalRocksDB<Description>
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

impl<Description> IterableStore for InnerHistoricalRocksDB<Description>
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
    for InnerHistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    fn commit_changes(
        &self,
        height: Option<Description::Height>,
        changes: Changes,
    ) -> StorageResult<()> {
        // cumulative_changes_lock_guard is defined to be Some only when the migration is in progress.
        // If the migration is not in progress, the default set of changes will be used, and the overhead
        // for handling caused by this function to handle the migration will be minimal.
        let (cumulative_changes, maybe_cumulative_changes_lock_guard) =
            self.take_migration_changes();

        let mut migration_transaction = StorageTransaction::transaction(
            &self.db,
            ConflictPolicy::Overwrite,
            // We need to keep ownership of the migration changes in case the current transaction fails.
            // In this case we need to place the cumulative changes for migrating the modification history
            // back into the lock guard.
            // Note: this might be requiring cloning a fair amount of data
            cumulative_changes.clone(),
        );
        let mut storage_transaction = StorageTransaction::transaction(
            &mut migration_transaction,
            ConflictPolicy::Overwrite,
            changes,
        );

        if let Some(height) = height {
            self.store_modifications_history(&mut storage_transaction, &height)?;
        }

        // This cannot fail
        storage_transaction.commit()?;

        // This can fail. In this case, we need to rollback place the cumulative migration history
        // changes back into the lock guard.
        match self
            .db
            .commit_changes(&migration_transaction.into_changes())
        {
            Ok(()) => Ok(()),
            Err(err) => {
                tracing::error!("Could not commit to historical RocksDB: {err:?}");
                self.add_migration_changes(
                    maybe_cumulative_changes_lock_guard,
                    cumulative_changes,
                )?;
                Err(err)
            }
        }
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
    use std::collections::HashMap;

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
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange)
                .unwrap();

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
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange)
                .unwrap();

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
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

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
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::NoRewind).unwrap();

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
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
    fn state_rewind_policy__rewind_range_1__rollback_uses_v2() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = InnerHistoricalRocksDB::new(
            rocks_db,
            StateRewindPolicy::RewindRange {
                size: NonZeroU64::new(1).unwrap(),
            },
        )
        .unwrap();

        // when
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(1u32.into()), transaction.into_changes())
            .unwrap();
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .collect::<Vec<_>>();
        // then

        assert_eq!(v2_entries.len(), 1);
        assert_eq!(v1_entries.len(), 0);
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__rollback_during_migration_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
            .commit_changes(&migration_transaction.into_changes())
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

        let result = historical_rocks_db.rollback_last_block();

        // Then
        assert_eq!(result, Ok(1));
        let v2_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        let v1_entries = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .collect::<Vec<_>>();
        assert_eq!(v2_entries.len(), 0);
        assert_eq!(v1_entries.len(), 0);
    }

    #[test]
    fn rollback_last_block_works_with_v2() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();

        let historical_rocks_db =
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange)
                .unwrap();

        // Commit 1000 blocks
        for i in 1..=1000u32 {
            let mut transaction = historical_rocks_db.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key(), &(123 + i as u64))
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(i.into()), transaction.into_changes())
                .unwrap();
        }

        // We can now rollback the last block 1000 times.
        // If the rollback fails at some point, then we have unintentionally rollbacked to
        // a block that was not the last.
        for i in 0..1000u32 {
            let result = historical_rocks_db.rollback_last_block();
            assert_eq!(result, Ok(1000 - i as u64));
        }
    }

    #[test]
    fn migrate_modifications_history_works() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
            .commit_changes(&migration_transaction.into_changes())
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

        // Migrate back from V1 to V2, using the provided function
        historical_rocks_db
            .migrate_modifications_history_at_height(1u64)
            .unwrap();

        // Check that the changes are not written to the database yet
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

        // Check that the migration is still in progress.
        assert!(historical_rocks_db.is_migration_in_progress());

        // Flush the changes to the DB
        historical_rocks_db.commit_migration_changes().unwrap();

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

        assert!(!historical_rocks_db.is_migration_in_progress());
    }

    #[tokio::test]
    async fn historical_rocksdb_perform_migration_on_new_instance() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();

        let historical_rocks_db =
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange)
                .unwrap();

        // Commit 100 blocks
        for i in 1..=100u32 {
            let mut transaction = historical_rocks_db.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key(), &(123 + i as u64))
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(i.into()), transaction.into_changes())
                .unwrap();
        }

        let mut revert_migration_transaction = StorageTransaction::transaction(
            &historical_rocks_db.db,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        // Revert the modification history from v2 to v1
        historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .map(Result::unwrap)
            .for_each(|(height, changes)| {
                revert_migration_transaction
                    .storage_as_mut::<ModificationsHistoryV1<OnChain>>()
                    .insert(&height, &changes)
                    .unwrap();
                revert_migration_transaction
                    .storage_as_mut::<ModificationsHistoryV2<OnChain>>()
                    .remove(&height)
                    .unwrap();
            });

        historical_rocks_db
            .db
            .commit_changes(&revert_migration_transaction.into_changes())
            .unwrap();

        let v1_changes = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .count();
        let v2_changes = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .count();

        assert_eq!(v1_changes, 100);
        assert_eq!(v2_changes, 0);

        // When
        // Wrap the inner historical rocksdb in a new instance to start the migration.
        let historical_rocks_db_with_migration =
            HistoricalRocksDB::try_from(historical_rocks_db).unwrap();

        // Keep writing to the database until the migration is complete.
        let mut i = 101;
        while historical_rocks_db_with_migration
            .inner
            .is_migration_in_progress()
        {
            let mut transaction = historical_rocks_db_with_migration.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key(), &(123 + i as u64))
                .unwrap();
            historical_rocks_db_with_migration
                .commit_changes(Some(i.into()), transaction.into_changes())
                .unwrap();
            i += 1;
        }
        // Then
        assert!(!historical_rocks_db_with_migration
            .inner
            .is_migration_in_progress());
    }

    #[tokio::test]
    async fn historical_rocksdb_migration_and_rollbacks_no_lost_updates() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();

        let historical_rocks_db =
            InnerHistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange)
                .unwrap();

        // Commit 1000 blocks
        for i in 1..=1000u32 {
            let mut transaction = historical_rocks_db.read_transaction();
            transaction
                .storage_as_mut::<ContractsAssets>()
                .insert(&key(), &(123 + i as u64))
                .unwrap();
            historical_rocks_db
                .commit_changes(Some(i.into()), transaction.into_changes())
                .unwrap();
        }

        let mut revert_migration_transaction = StorageTransaction::transaction(
            &historical_rocks_db.db,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        // Revert the modification history from v2 to v1
        historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .map(Result::unwrap)
            .for_each(|(height, changes)| {
                revert_migration_transaction
                    .storage_as_mut::<ModificationsHistoryV1<OnChain>>()
                    .insert(&height, &changes)
                    .unwrap();
                revert_migration_transaction
                    .storage_as_mut::<ModificationsHistoryV2<OnChain>>()
                    .remove(&height)
                    .unwrap();
            });

        historical_rocks_db
            .db
            .commit_changes(&revert_migration_transaction.into_changes())
            .unwrap();

        let v1_changes_before_migration: HashMap<_, _> = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV1<OnChain>>(None)
            .map(Result::unwrap)
            .collect();

        let v2_changes_count = historical_rocks_db
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .count();

        assert_eq!(v1_changes_before_migration.len(), 1000);
        assert_eq!(v2_changes_count, 0);

        // When
        // Wrap the inner historical rocksdb in a new instance to start the migration.
        let historical_rocks_db_with_migration =
            HistoricalRocksDB::try_from(historical_rocks_db).unwrap();

        // Keep writing to the database until the migration is complete.
        while historical_rocks_db_with_migration
            .inner
            .is_migration_in_progress()
        {
            if let Err(_) = historical_rocks_db_with_migration
                .inner
                .rollback_last_block()
            {
                // If rolling back fails, then cumulative changes are not being committed to rocksDB.
                // We flush them the DB to keep the migration going.
                // In a real scenario, the migration will progress because we always advance the height
                // by committing new blocks.
                historical_rocks_db_with_migration
                    .inner
                    .commit_migration_changes()
                    .unwrap();
            }
        }
        // Then
        assert!(!historical_rocks_db_with_migration
            .inner
            .is_migration_in_progress());

        let v2_changes: HashMap<_, _> = historical_rocks_db_with_migration
            .inner
            .db
            .iter_all::<ModificationsHistoryV2<OnChain>>(None)
            .map(Result::unwrap)
            .collect();

        // Check that all the keys that have not been rolled back are consistent with V1
        for (column, changes) in v2_changes {
            assert_eq!(changes, *v1_changes_before_migration.get(&column).unwrap());
        }
    }

    #[test]
    fn state_rewind_policy__rewind_range_1__second_rollback_fails() {
        // Given
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
        let historical_rocks_db = InnerHistoricalRocksDB::new(
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
