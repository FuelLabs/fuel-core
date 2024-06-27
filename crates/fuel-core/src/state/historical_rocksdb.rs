use crate::{
    database::{
        database_description::{
            DatabaseDescription,
            DatabaseHeight,
        },
        Result as DatabaseResult,
    },
    state::{
        iterable_view::IterableViewWrapper,
        key_value_view::KeyValueViewWrapper,
        rocks_db::RocksDb,
        ColumnType,
        IterableView,
        KeyValueView,
        TransactableStorage,
    },
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    structured_storage::TableWithBlueprint,
    transactional::{
        Changes,
        ConflictPolicy,
        ReadTransaction,
        StorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
};
use itertools::Itertools;
use rocksdb::{
    IteratorMode,
    ReadOptions,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::num::NonZeroU64;

#[derive(Debug, Copy, Clone, strum_macros::EnumCount, enum_iterator::Sequence)]
enum HistoryColumn<Description>
where
    Description: DatabaseDescription,
{
    UnderlyingDatabase(Description::Column),
    HistoryColumn,
}

impl<Description> StorageColumn for HistoryColumn<Description>
where
    Description: DatabaseDescription,
{
    fn name(&self) -> &'static str {
        match self {
            HistoryColumn::UnderlyingDatabase(c) => c.name(),
            HistoryColumn::HistoryColumn => "history",
        }
    }

    fn id(&self) -> u32 {
        match self {
            HistoryColumn::UnderlyingDatabase(c) => c.id(),
            HistoryColumn::HistoryColumn => u32::MAX,
        }
    }
}

struct ModificationHistory<Description>(core::marker::PhantomData<Description>)
where
    Description: DatabaseDescription;

impl<Description> Mappable for ModificationHistory<Description>
where
    Description: DatabaseDescription,
{
    /// The height of the modifications.
    type Key = u64;
    type OwnedKey = Self::Key;
    /// Reverse modification at the corresponding height.
    type Value = Changes;
    type OwnedValue = Self::Value;
}

impl<Description> TableWithBlueprint for ModificationHistory<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = HistoryColumn<Description>;

    fn column() -> Self::Column {
        HistoryColumn::HistoryColumn
    }
}

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

#[derive(Debug, Copy, Clone)]
struct HistoricalDescription<Description> {
    _maker: core::marker::PhantomData<Description>,
}

impl<Description> DatabaseDescription for HistoricalDescription<Description>
where
    Description: DatabaseDescription,
{
    type Column = HistoryColumn<Description>;
    type Height = Description::Height;

    fn version() -> u32 {
        Description::version()
    }

    fn name() -> &'static str {
        Description::name()
    }

    fn metadata_column() -> Self::Column {
        HistoryColumn::UnderlyingDatabase(Description::metadata_column())
    }

    fn prefix(column: &Self::Column) -> Option<usize> {
        let prefix = match column {
            HistoryColumn::UnderlyingDatabase(c) => Description::prefix(c),
            HistoryColumn::HistoryColumn => None,
        }
        .unwrap_or(0)
        .saturating_add(8); // `u64::to_be_bytes`

        Some(prefix)
    }
}

#[derive(Debug)]
pub struct HistoricalRocksDB<Description> {
    state_rewind_policy: StateRewindPolicy,
    db: RocksDb<Description>,
    history: RocksDb<HistoricalDescription<Description>>,
}

impl<Description> HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(
        db: RocksDb<Description>,
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        let path = db.path().join("history");
        let history = RocksDb::default_open(path, None)?;
        Ok(Self {
            state_rewind_policy,
            db,
            history,
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

    pub fn latest_view(&self) -> StorageResult<RocksDb<Description>> {
        let db_view = self.db.create_snapshot();
        Ok(db_view)
    }

    pub fn create_view_at(
        &self,
        height: &Description::Height,
    ) -> StorageResult<ViewAtHeight<Description>> {
        let latest_view = self.latest_view()?;

        // Each height stores reverse modification caused by the corresponding
        // block at the same height. Applying reverse changes at height `X`
        // gives us a state at height `X - 1`. If we want a state at height `X`,
        // we need to apply all modifications up to `X + 1`.
        let rollback_height = height.as_u64().saturating_add(1);
        let history_view = self.history.create_snapshot();

        Ok(ViewAtHeight {
            height: rollback_height,
            read_db: latest_view,
            history: history_view,
        })
    }

    fn store_modifications_history(
        &self,
        height: &Description::Height,
        changes: &Changes,
    ) -> StorageResult<()> {
        if self.state_rewind_policy == StateRewindPolicy::NoRewind {
            return Ok(());
        }

        let height_u64 = height.as_u64();
        let mut storage_transaction = self.history.read_transaction();

        cleanup_old_changes(
            &height_u64,
            &mut storage_transaction,
            &self.state_rewind_policy,
        )?;

        let reverse_changes = self.reverse_history_changes(changes)?;
        let old_changes = storage_transaction
            .storage_as_mut::<ModificationHistory<Description>>()
            .insert(&height_u64, &reverse_changes)?;

        // The existence of old changes at the current height means we
        // try to override the current height. So, we need to remove
        // all historical modifications from the past.
        if let Some(old_changes) = old_changes {
            remove_historical_modifications(
                &height_u64,
                &mut storage_transaction,
                old_changes,
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
                Ok::<_, StorageError>((column, historical_column_changes))
            })
            .try_collect()?;

        // Combine removed old changes, all modifications for
        // the current height and historical changes.
        StorageTransaction::transaction(
            &mut storage_transaction,
            ConflictPolicy::Overwrite,
            historical_changes,
        )
        .commit()?;

        let changes = storage_transaction.into_changes();
        self.history.commit_changes(&changes)
    }
}

fn cleanup_old_changes<Description>(
    height: &u64,
    storage_transaction: &mut StorageTransaction<
        &RocksDb<HistoricalDescription<Description>>,
    >,
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

            let old_changes = storage_transaction
                .storage_as_mut::<ModificationHistory<Description>>()
                .remove(&old_height)?;

            if let Some(old_changes) = old_changes {
                remove_historical_modifications(
                    &old_height,
                    storage_transaction,
                    old_changes,
                )?;
            }
        }
    }
    Ok(())
}

fn remove_historical_modifications<Description>(
    old_height: &u64,
    storage_transaction: &mut StorageTransaction<
        &RocksDb<HistoricalDescription<Description>>,
    >,
    reverse_changes: Changes,
) -> StorageResult<()>
where
    Description: DatabaseDescription,
{
    let changes = reverse_changes
        .into_iter()
        .map(|(column, column_changes)| {
            let historical_column_changes = column_changes
                .into_iter()
                .map(|(key, _)| {
                    let height_key = height_key(&key, old_height).into();
                    let operation = WriteOperation::Remove;
                    (height_key, operation)
                })
                .collect();
            (column, historical_column_changes)
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
        self.db.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.db.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.db.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.db.read(key, column, buf)
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
        self.db.iter_store(column, prefix, start, direction)
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
        if let Some(height) = height {
            self.store_modifications_history(&height, &changes)?;
        }

        self.db.commit_changes(&changes)?;
        Ok(())
    }

    fn view_at_height(
        &self,
        height: &Description::Height,
    ) -> StorageResult<KeyValueView<ColumnType<Description>>> {
        let view = self.create_view_at(height)?;
        Ok(KeyValueView::from_storage(KeyValueViewWrapper::new(view)))
    }

    fn latest_view(&self) -> StorageResult<IterableView<ColumnType<Description>>> {
        let view = self.latest_view()?;
        Ok(IterableView::from_storage(IterableViewWrapper::new(view)))
    }
}

pub struct ViewAtHeight<Description> {
    height: u64,
    read_db: RocksDb<Description>,
    history: RocksDb<HistoricalDescription<Description>>,
}

impl<Description> KeyValueInspect for ViewAtHeight<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let read_history = &self.history;
        let height_key = height_key(key, &self.height);
        let options = ReadOptions::default();
        let nearest_modification = read_history
            ._iter_all(
                HistoryColumn::UnderlyingDatabase(column),
                options,
                IteratorMode::From(&height_key, rocksdb::Direction::Forward),
            )
            .next();

        if let Some(upper_bound) = nearest_modification {
            let (found_height_key, value) = upper_bound?;
            let found_key = found_height_key.as_slice();

            if &found_key[..key.len()] == key {
                let value = deserialize(&value)?;

                return match value {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
        }

        self.read_db.get(key, column)
    }
}

pub fn height_key(key: &[u8], height: &u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(key.len() + 8);
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
        let rocks_db = RocksDb::<OnChain>::default_open_temp(None).unwrap();
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
        let latest_view = historical_rocks_db.read_transaction();
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
    fn historical_rocksdb_read_latest_view_works() {
        // Given
        let rocks_db = RocksDb::<OnChain>::default_open_temp(None).unwrap();
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
        let latest_view = historical_rocks_db
            .latest_view()
            .unwrap()
            .into_transaction();
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
    fn historical_rocksdb_view_at_each_height_works() {
        // Given
        let rocks_db = RocksDb::<OnChain>::default_open_temp(None).unwrap();
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
        let view_at_height_zero = historical_rocks_db
            .create_view_at(&0u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_one = historical_rocks_db
            .create_view_at(&1u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_two = historical_rocks_db
            .create_view_at(&2u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_three = historical_rocks_db
            .create_view_at(&3u32.into())
            .unwrap()
            .into_transaction();
        let balance_at_height_zero = view_at_height_zero
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap();
        let balance_at_height_one = view_at_height_one
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();
        let balance_at_height_two = view_at_height_two
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();
        let balance_at_height_three = view_at_height_three
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(balance_at_height_zero, None);
        assert_eq!(balance_at_height_one, 123);
        assert_eq!(balance_at_height_two, 321);
        assert_eq!(balance_at_height_three, 321);
    }

    #[test]
    fn historical_rocksdb_view_at_each_height_works_when_multiple_modifications() {
        // Given
        let rocks_db = RocksDb::<OnChain>::default_open_temp(None).unwrap();
        let historical_rocks_db =
            HistoricalRocksDB::new(rocks_db, StateRewindPolicy::RewindFullRange).unwrap();

        // Set the value at height 1 to be 123.
        let mut transaction = historical_rocks_db.read_transaction();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&key(), &123)
            .unwrap();
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&Default::default(), &123456)
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
        transaction
            .storage_as_mut::<ContractsAssets>()
            .insert(&Default::default(), &654321)
            .unwrap();
        historical_rocks_db
            .commit_changes(Some(2u32.into()), transaction.into_changes())
            .unwrap();

        // When
        let view_at_height_zero = historical_rocks_db
            .create_view_at(&0u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_one = historical_rocks_db
            .create_view_at(&1u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_two = historical_rocks_db
            .create_view_at(&2u32.into())
            .unwrap()
            .into_transaction();
        let view_at_height_three = historical_rocks_db
            .create_view_at(&3u32.into())
            .unwrap()
            .into_transaction();
        let balance_at_height_zero = view_at_height_zero
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap();
        let balance_at_height_one = view_at_height_one
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();
        let balance_at_height_two = view_at_height_two
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();
        let balance_at_height_three = view_at_height_three
            .storage_as_ref::<ContractsAssets>()
            .get(&key())
            .unwrap()
            .unwrap()
            .into_owned();

        // Then
        assert_eq!(balance_at_height_zero, None);
        assert_eq!(balance_at_height_one, 123);
        assert_eq!(balance_at_height_two, 321);
        assert_eq!(balance_at_height_three, 321);
    }
}
