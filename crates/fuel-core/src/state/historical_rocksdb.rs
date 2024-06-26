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
        TransactableHistoricalStorage,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        Value,
        WriteOperation,
    },
    structured_storage::StructuredStorage,
    transactional::Changes,
    Error as StorageError,
    Result as StorageResult,
};
use rocksdb::{
    IteratorMode,
    ReadOptions,
};
use std::sync::Arc;

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
    RewindRange { size: u64 },
}

#[derive(Debug, Clone)]
struct HistoricalDescription<Description> {
    _maker: core::marker::PhantomData<Description>,
}

impl<Description> DatabaseDescription for HistoricalDescription<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;
    type Height = Description::Height;

    fn version() -> u32 {
        Description::version()
    }

    fn name() -> &'static str {
        Description::name()
    }

    fn metadata_column() -> Self::Column {
        Description::metadata_column()
    }

    fn prefix(column: &Self::Column) -> Option<usize> {
        Some(
            Description::prefix(column).unwrap_or(0).saturating_add(8), /* `u64::to_be_bytes` */
        )
    }
}

#[derive(Debug)]
pub struct HistoricalRocksDB<Description> {
    db: RocksDb<Description>,
    history: RocksDb<HistoricalDescription<Description>>,
}

impl<Description> HistoricalRocksDB<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(db: RocksDb<Description>, _: StateRewindPolicy) -> DatabaseResult<Self> {
        let path = db.path().join("history");
        let history = RocksDb::default_open(path, None)?;
        Ok(Self { db, history })
    }

    fn reverse_history_changes(
        &self,
        changes: &Changes,
        height: &Description::Height,
    ) -> StorageResult<Changes> {
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
                        entry.insert(
                            height_key(key, height.as_u64()).into(),
                            WriteOperation::Insert(serialize_operation(
                                WriteOperation::Remove,
                            )?),
                        );
                    }
                    (Some(old_value), WriteOperation::Remove) => {
                        entry.insert(
                            height_key(key, height.as_u64()).into(),
                            WriteOperation::Insert(serialize_operation(
                                WriteOperation::Insert(old_value.into()),
                            )?),
                        );
                    }
                    (Some(old_value), WriteOperation::Insert(new_value)) => {
                        if old_value.as_slice() != new_value.as_slice() {
                            entry.insert(
                                height_key(key, height.as_u64()).into(),
                                WriteOperation::Insert(serialize_operation(
                                    WriteOperation::Insert(old_value.into()),
                                )?),
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

        // Each height stores modifications caused by the corresponding block
        // at the same height. So if we need to view at height `X`,
        // we need to roll back modifications caused by all the blocks before `X`.
        let rollback_height = height.as_u64().saturating_add(1);
        let history_view = self.history.create_snapshot();

        Ok(ViewAtHeight {
            height: rollback_height,
            read_db: latest_view,
            history: history_view,
        })
    }
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

impl<Description> TransactableHistoricalStorage<Description::Height>
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
            let reverse_changes = self.reverse_history_changes(&changes, &height)?;
            self.history.commit_changes(&reverse_changes)?;
        }

        // TODO: Handle the case when `history.commit_changes` is successful,
        //  but `db.commit_changes` is not.
        self.db.commit_changes(&changes)?;
        Ok(())
    }

    fn view_at_height(
        &self,
        height: &Description::Height,
    ) -> StorageResult<KeyValueView<ColumnType<Description>>> {
        let view = self.create_view_at(height)?;
        Ok(StructuredStorage::new(KeyValueViewWrapper::new(Arc::new(view))).into())
    }

    fn latest_view(&self) -> StorageResult<IterableView<ColumnType<Description>>> {
        let view = self.latest_view()?;
        Ok(StructuredStorage::new(IterableViewWrapper::new(Arc::new(view))).into())
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
        let height_key = height_key(key, self.height);
        let options = ReadOptions::default();
        let nearest_modification = read_history
            ._iter_all(
                column,
                options,
                IteratorMode::From(&height_key, rocksdb::Direction::Forward),
            )
            .next();

        if let Some(upper_bound) = nearest_modification {
            let (found_height_key, value) = upper_bound?;
            let found_key = found_height_key.as_slice();

            if &found_key[..key.len()] == key {
                let value = deserialize_operation(&value)?;

                return match value {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
        }

        self.read_db.get(key, column)
    }
}

pub fn height_key(key: &[u8], height: u64) -> Vec<u8> {
    let height_bytes = height.to_be_bytes();
    let mut bytes = key.to_vec();
    bytes.extend_from_slice(&height_bytes);
    bytes
}

pub fn serialize_operation(operation: WriteOperation) -> StorageResult<Value> {
    Ok(postcard::to_allocvec(&operation)
        .map_err(|err| StorageError::Codec(err.into()))?
        .into())
}

pub fn deserialize_operation(value: &[u8]) -> StorageResult<WriteOperation> {
    postcard::from_bytes(value).map_err(|err| StorageError::Codec(err.into()))
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
        assert_eq!(height_key(key, height), expected);
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
