use crate::{
    database::database_description::DatabaseDescription,
    state::{
        historical_rocksdb::{
            description::{
                Column,
                Historical,
            },
            deserialize,
            height_key,
        },
        rocks_db::{
            KeyAndValue,
            RocksDb,
        },
    },
};
use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        Value,
        WriteOperation,
    },
    Result as StorageResult,
};
use rocksdb::{
    IteratorMode,
    ReadOptions,
};

pub struct ViewAtHeight<Description> {
    height: u64,
    read_db: RocksDb<Historical<Description>>,
}

impl<Description> ViewAtHeight<Description>
where
    Description: DatabaseDescription,
{
    pub fn new(height: u64, read_db: RocksDb<Historical<Description>>) -> Self {
        Self { height, read_db }
    }
}

impl<Description> KeyValueInspect for ViewAtHeight<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let read_history = &self.read_db;
        let height_key = height_key(key, &self.height);
        let options = ReadOptions::default();
        let nearest_modification = read_history
            .iterator::<KeyAndValue>(
                Column::HistoricalDuplicateColumn(column),
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

        self.read_db.get(key, Column::OriginalColumn(column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        database::database_description::on_chain::OnChain,
        state::{
            historical_rocksdb::{
                HistoricalRocksDB,
                StateRewindPolicy,
            },
            TransactableStorage,
        },
    };
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

    fn key() -> ContractsAssetKey {
        ContractsAssetKey::new(&[123; 32].into(), &[213; 32].into())
    }

    #[test]
    fn historical_rocksdb_view_at_each_height_works() {
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
        let rocks_db = RocksDb::<Historical<OnChain>>::default_open_temp(None).unwrap();
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
