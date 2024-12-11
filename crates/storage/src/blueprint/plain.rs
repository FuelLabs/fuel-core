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
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    kv_store::{
        BatchOperations,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
        WriteOperation,
    },
    structured_storage::TableWithBlueprint,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
};

/// The type that represents the plain blueprint.
/// The `KeyCodec` and `ValueCodec` are used to encode/decode the key and value.
pub struct Plain<KeyCodec, ValueCodec> {
    _marker: core::marker::PhantomData<(KeyCodec, ValueCodec)>,
}

impl<M, S, KeyCodec, ValueCodec> BlueprintInspect<M, S> for Plain<KeyCodec, ValueCodec>
where
    M: Mappable,
    S: KeyValueInspect,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
{
    type KeyCodec = KeyCodec;
    type ValueCodec = ValueCodec;
}

impl<M, S, KeyCodec, ValueCodec> BlueprintMutate<M, S> for Plain<KeyCodec, ValueCodec>
where
    M: Mappable,
    S: KeyValueMutate,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
{
    fn put(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<()> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = ValueCodec::encode_as_value(value);
        storage.put(key_bytes.as_ref(), column, value)
    }

    fn replace(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = ValueCodec::encode_as_value(value);
        storage
            .replace(key_bytes.as_ref(), column, value)?
            .map(|value| {
                ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()
    }

    fn take(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage
            .take(key_bytes.as_ref(), column)?
            .map(|value| {
                ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()
    }

    fn delete(storage: &mut S, key: &M::Key, column: S::Column) -> StorageResult<()> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.delete(key_bytes.as_ref(), column)
    }
}

impl<Column, M, S, KeyCodec, ValueCodec> SupportsBatching<M, S>
    for Plain<KeyCodec, ValueCodec>
where
    Column: StorageColumn,
    S: BatchOperations<Column = Column>,
    M: TableWithBlueprint<Blueprint = Plain<KeyCodec, ValueCodec>, Column = Column>,
    M::Blueprint: BlueprintMutate<M, S>,
{
    fn init<'a, Iter>(storage: &mut S, column: S::Column, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        Self::insert(storage, column, set)
    }

    fn insert<'a, Iter>(
        storage: &mut S,
        column: S::Column,
        set: Iter,
    ) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        storage.batch_write(
            column,
            set.map(|(key, value)| {
                let key_encoder =
                    <M::Blueprint as BlueprintInspect<M, S>>::KeyCodec::encode(key);
                let key_bytes = key_encoder.as_bytes().to_vec();
                let value =
                    <M::Blueprint as BlueprintInspect<M, S>>::ValueCodec::encode_as_value(
                        value,
                    );
                (key_bytes, WriteOperation::Insert(value))
            }),
        )
    }

    fn remove<'a, Iter>(
        storage: &mut S,
        column: S::Column,
        set: Iter,
    ) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        storage.batch_write(
            column,
            set.map(|key| {
                let key_encoder =
                    <M::Blueprint as BlueprintInspect<M, S>>::KeyCodec::encode(key);
                let key_bytes = key_encoder.as_bytes().to_vec();
                (key_bytes, WriteOperation::Remove)
            }),
        )
    }
}

/// The macro that generates basic storage tests for the table with the plain structure.
/// It uses the [`InMemoryStorage`](crate::structured_storage::memory::InMemoryStorage).
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
                structured_storage::memory::InMemoryStorage,
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

            #[test]
            fn batch_mutate_works() {
                use $crate::rand::{
                    Rng,
                    rngs::StdRng,
                    RngCore,
                    SeedableRng,
                };

                let empty_storage = InMemoryStorage::default();

                let mut init_storage = InMemoryStorage::default();
                let mut init_structured_storage = init_storage.write_transaction();

                let mut rng = &mut StdRng::seed_from_u64(1234);
                let gen = || Some($random_key(&mut rng));
                let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
                let value = $value_insert;

                <_ as $crate::StorageBatchMutate<$table>>::init_storage(
                    &mut init_structured_storage,
                    &mut data.iter().map(|k| {
                        let value: &<$table as $crate::Mappable>::Value = &value;
                        (k, value)
                    })
                ).expect("Should initialize the storage successfully");
                init_structured_storage.commit().expect("Should commit changes");

                let mut insert_storage = InMemoryStorage::default();
                let mut insert_structured_storage = insert_storage.write_transaction();

                <_ as $crate::StorageBatchMutate<$table>>::insert_batch(
                    &mut insert_structured_storage,
                    &mut data.iter().map(|k| {
                        let value: &<$table as $crate::Mappable>::Value = &value;
                        (k, value)
                    })
                ).expect("Should insert batch successfully");
                insert_structured_storage.commit().expect("Should commit changes");

                assert_eq!(init_storage, insert_storage);
                assert_ne!(init_storage, empty_storage);
                assert_ne!(insert_storage, empty_storage);

                let mut remove_from_insert_structured_storage = insert_storage.write_transaction();
                <_ as $crate::StorageBatchMutate<$table>>::remove_batch(
                    &mut remove_from_insert_structured_storage,
                    &mut data.iter()
                ).expect("Should remove all entries successfully from insert storage");
                remove_from_insert_structured_storage.commit().expect("Should commit changes");
                assert_ne!(init_storage, insert_storage);
                assert_eq!(insert_storage, empty_storage);

                let mut remove_from_init_structured_storage = init_storage.write_transaction();
                <_ as $crate::StorageBatchMutate<$table>>::remove_batch(
                    &mut remove_from_init_structured_storage,
                    &mut data.iter()
                ).expect("Should remove all entries successfully from init storage");
                remove_from_init_structured_storage.commit().expect("Should commit changes");
                assert_eq!(init_storage, insert_storage);
                assert_eq!(init_storage, empty_storage);
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
