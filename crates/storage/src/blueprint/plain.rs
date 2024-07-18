//! This module implements the plain blueprint for the storage.
//! The plain blueprint is the simplest one. It doesn't maintain any additional data structures
//! and doesn't provide any additional functionality. It is just a key-value store that encodes/decodes
//! the key and value and puts/takes them into/from the storage.

use crate::{
    blueprint::{
        BlueprintInspect,
        BlueprintMutate,
        SupportsBatching,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
};
use std::borrow::Cow;

/// The type that represents the plain blueprint.
pub struct Plain;

impl<M, S> BlueprintInspect<M, S> for Plain
where
    M: Mappable,
    S: StorageInspect<M, Error = StorageError>,
{
    fn exists(storage: &S, key: &M::Key) -> StorageResult<bool> {
        storage.storage_as_ref::<M>().contains_key(key)
    }

    fn get<'a>(
        storage: &'a S,
        key: &M::Key,
    ) -> StorageResult<Option<Cow<'a, M::OwnedValue>>> {
        storage.storage_as_ref::<M>().get(key)
    }
}

impl<M, S> BlueprintMutate<M, S> for Plain
where
    M: Mappable,
    S: StorageMutate<M, Error = StorageError>,
{
    fn put(storage: &mut S, key: &M::Key, value: &M::Value) -> StorageResult<()> {
        storage.storage_as_mut().insert(key, value)
    }

    fn replace(
        storage: &mut S,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        storage.storage_as_mut().replace(key, value)
    }

    fn take(storage: &mut S, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        storage.storage_as_mut().take(key)
    }

    fn delete(storage: &mut S, key: &M::Key) -> StorageResult<()> {
        storage.storage_as_mut().remove(key)
    }
}

impl<M, S> SupportsBatching<M, S> for Plain
where
    M: Mappable,
    S: StorageBatchMutate<M, Error = StorageError>,
{
    fn init<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        storage.init_storage(set)
    }

    fn insert<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        storage.insert_batch(set)
    }

    fn remove<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        storage.remove_batch(set)
    }
}

/// The macro that generates basic storage tests for the table with the plain structure.
/// It uses the [`InMemoryStorage`](crate::structured_storage::test::InMemoryStorage).
#[cfg(feature = "test-helpers")]
#[macro_export]
macro_rules! basic_storage_tests {
    ($table:ident, $key:expr, $value_insert:expr, $value_return:expr, $random_key:expr) => {
        $crate::paste::item! {
        #[cfg(test)]
        #[allow(unused_imports)]
        mod [< $table:snake _basic_tests >] {
            use super::*;
            use $crate::{
                structured_storage::test::InMemoryStorage,
                transactional::WriteTransaction,
                StorageAsMut,
            };
            use $crate::StorageInspect;
            use $crate::StorageMutate;
            use $crate::rand;

            #[allow(dead_code)]
            fn random<T, R>(rng: &mut R) -> T
            where
                rand::distributions::Standard: rand::distributions::Distribution<T>,
                R: rand::Rng,
            {
                use rand::Rng;
                rng.gen()
            }

            #[test]
            fn get() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                let _: &dyn StorageMutate<$table, Error = $crate::Error> = &storage_transaction;
                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                assert_eq!(
                    storage_transaction
                        .storage_as_mut::<$table>()
                        .get(&key)
                        .expect("Should get without errors")
                        .expect("Should not be empty")
                        .into_owned(),
                    $value_return
                );
            }

            #[test]
            fn insert() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                let returned = storage_transaction
                    .storage_as_mut::<$table>()
                    .get(&key)
                    .unwrap()
                    .unwrap()
                    .into_owned();
                assert_eq!(returned, $value_return);
            }

            #[test]
            fn remove() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                storage_transaction.storage_as_mut::<$table>().remove(&key).unwrap();

                assert!(!storage_transaction
                    .storage_as_mut::<$table>()
                    .contains_key(&key)
                    .unwrap());
            }

            #[test]
            fn exists() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                // Given
                assert!(!storage_transaction
                    .storage_as_mut::<$table>()
                    .contains_key(&key)
                    .unwrap());

                // When
                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                // Then
                assert!(storage_transaction
                    .storage_as_mut::<$table>()
                    .contains_key(&key)
                    .unwrap());
            }

            #[test]
            fn exists_false_after_removing() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                // Given
                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                // When
                storage_transaction
                    .storage_as_mut::<$table>()
                    .remove(&key)
                    .unwrap();

                // Then
                assert!(!storage_transaction
                    .storage_as_mut::<$table>()
                    .contains_key(&key)
                    .unwrap());
            }
        }}
    };
    ($table:ident, $key:expr, $value_insert:expr, $value_return:expr) => {
        $crate::basic_storage_tests!($table, $key, $value_insert, $value_return, random);
    };
    ($table:ident, $key:expr, $value:expr) => {
        $crate::basic_storage_tests!($table, $key, $value, $value);
    };
}
