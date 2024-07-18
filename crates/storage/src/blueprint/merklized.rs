//! The module defines the `Merklized` blueprint for the storage.
//! The `Merklized` blueprint implements the binary merkle tree on top of the storage
//! for all entries.

use crate::{
    blueprint::{
        BlueprintInspect,
        BlueprintMutate,
        SupportsBatching,
        SupportsMerkle,
    },
    codec::{
        Encode,
        Encoder as EncoderTrait,
    },
    not_found,
    tables::merkle::{
        DenseMerkleMetadata,
        DenseMerkleMetadataV1,
        DenseMetadataKey,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::binary::Primitive;
use std::borrow::Cow;

/// The `Merklized` blueprint builds the storage as a [`Plain`](super::plain::Plain)
/// blueprint and maintains the binary merkle tree by the `Metadata` table.
///
/// The `Metadata` table stores the metadata of the binary merkle tree(like a root of the tree and leaves count).
///
/// The `ValueEncoder` is used to encode the value for merklelization.
pub struct Merklized<Metadata, Nodes, ValueEncoder> {
    _marker: core::marker::PhantomData<(Metadata, Nodes, ValueEncoder)>,
}

impl<Metadata, Nodes, Encoder> Merklized<Metadata, Nodes, Encoder>
where
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
{
    fn insert_into_tree<S, M>(
        mut storage: &mut S,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<()>
    where
        M: Mappable,
        Metadata: Mappable<
            Key = DenseMetadataKey<M::OwnedKey>,
            Value = DenseMerkleMetadata,
            OwnedValue = DenseMerkleMetadata,
        >,
        Encoder: Encode<M::Value>,
        S: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
    {
        let key = key.to_owned().into();
        // Get latest metadata entry
        let prev_metadata = storage
            .storage::<Metadata>()
            .get(&DenseMetadataKey::Latest)?
            .unwrap_or_default();
        let previous_version = prev_metadata.version();

        let mut tree: fuel_core_types::fuel_merkle::binary::MerkleTree<Nodes, _> =
            fuel_core_types::fuel_merkle::binary::MerkleTree::load(
                &mut storage,
                previous_version,
            )
            .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;
        let encoder = Encoder::encode(value);
        tree.push(encoder.as_bytes().as_ref())?;

        // Generate new metadata for the updated tree
        let version = tree.leaves_count();
        let root = tree.root();
        let metadata = DenseMerkleMetadata::V1(DenseMerkleMetadataV1 { version, root });
        storage
            .storage::<Metadata>()
            .insert(&DenseMetadataKey::Primary(key), &metadata)?;
        // Duplicate the metadata entry for the latest key.
        storage
            .storage::<Metadata>()
            .insert(&DenseMetadataKey::Latest, &metadata)?;

        Ok(())
    }

    fn remove<S, M>(storage: &mut S, key: &M::Key) -> StorageResult<()>
    where
        S: StorageInspect<M, Error = StorageError>,
        M: Mappable,
    {
        if storage.contains_key(key)? {
            Err(anyhow::anyhow!(
                "It is not allowed to remove or override entries in the merklelized table"
            )
            .into())
        } else {
            Ok(())
        }
    }
}

impl<M, S, Metadata, Nodes, Encoder> BlueprintInspect<M, S>
    for Merklized<Metadata, Nodes, Encoder>
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

impl<M, S, Metadata, Nodes, Encoder> BlueprintMutate<M, S>
    for Merklized<Metadata, Nodes, Encoder>
where
    M: Mappable,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    Encoder: Encode<M::Value>,
    S: StorageMutate<M, Error = StorageError>
        + StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn put(storage: &mut S, key: &M::Key, value: &M::Value) -> StorageResult<()> {
        storage.storage_as_mut::<M>().insert(key, value)?;
        Self::insert_into_tree::<S, M>(storage, key, value)
    }

    fn replace(
        storage: &mut S,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let prev = storage.storage_as_mut::<M>().replace(key, value)?;

        if prev.is_some() {
            Self::remove::<S, M>(storage, key)?;
        }

        Self::insert_into_tree::<S, M>(storage, key, value)?;
        Ok(prev)
    }

    fn take(storage: &mut S, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        Self::remove::<S, M>(storage, key)?;
        let prev = storage.storage_as_mut::<M>().take(key)?;
        Ok(prev)
    }

    fn delete(storage: &mut S, key: &M::Key) -> StorageResult<()> {
        Self::remove::<S, M>(storage, key)?;
        storage.storage_as_mut::<M>().remove(key)
    }
}

impl<M, S, Metadata, Nodes, Encoder> SupportsMerkle<M::Key, M, S>
    for Merklized<Metadata, Nodes, Encoder>
where
    M: Mappable,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    S: StorageInspect<Metadata, Error = StorageError>,
    Self: BlueprintInspect<M, S>,
{
    fn root(storage: &S, key: &M::Key) -> StorageResult<MerkleRoot> {
        let key = key.to_owned().into();
        let metadata = storage
            .storage_as_ref::<Metadata>()
            .get(&DenseMetadataKey::Primary(key))?
            .ok_or(not_found!(Metadata))?;
        Ok(*metadata.root())
    }
}

impl<M, S, Metadata, Nodes, Encoder> SupportsBatching<M, S>
    for Merklized<Metadata, Nodes, Encoder>
where
    M: Mappable,
    Encoder: Encode<M::Value>,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    S: StorageMutate<M, Error = StorageError>
        + StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn init<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <Self as SupportsBatching<M, S>>::insert(storage, set)
    }

    fn insert<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        for (key, value) in set {
            <Self as BlueprintMutate<M, S>>::replace(storage, key, value)?;
        }

        Ok(())
    }

    fn remove<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        for key in set {
            Self::remove::<S, M>(storage, key)?;
        }
        Ok(())
    }
}

/// The macro that generates basic storage tests for the table with the merklelized structure.
/// It uses the [`InMemoryStorage`](crate::structured_storage::test::InMemoryStorage).
#[cfg(feature = "test-helpers")]
#[macro_export]
macro_rules! basic_merklelized_storage_tests {
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
            use $crate::tables::merkle::DenseMetadataKey;
            use rand::SeedableRng;

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
            fn remove_returns_error() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();
                let key = $key;

                storage_transaction
                    .storage_as_mut::<$table>()
                    .insert(&key, &$value_insert)
                    .unwrap();

                let result = storage_transaction.storage_as_mut::<$table>().remove(&key);

                assert!(result.is_err());
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
                init_structured_storage.commit().expect("Should commit the storage");

                let mut insert_storage = InMemoryStorage::default();
                let mut insert_structured_storage = insert_storage.write_transaction();

                <_ as $crate::StorageBatchMutate<$table>>::insert_batch(
                    &mut insert_structured_storage,
                    &mut data.iter().map(|k| {
                        let value: &<$table as $crate::Mappable>::Value = &value;
                        (k, value)
                    })
                ).expect("Should insert batch successfully");
                insert_structured_storage.commit().expect("Should commit the storage");

                assert_eq!(init_storage, insert_storage);
                assert_ne!(init_storage, empty_storage);
                assert_ne!(insert_storage, empty_storage);
            }

            #[test]
            fn batch_remove_fails() {
                use $crate::rand::{
                    Rng,
                    rngs::StdRng,
                    RngCore,
                    SeedableRng,
                };

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

                let result = <_ as $crate::StorageBatchMutate<$table>>::remove_batch(
                    &mut init_structured_storage,
                    &mut data.iter()
                );

                assert!(result.is_err());
            }

            #[test]
            fn root_returns_error_empty_metadata() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();

                let root = storage_transaction
                    .storage_as_mut::<$table>()
                    .root(&$key);
                assert!(root.is_err())
            }

            #[test]
            fn update_produces_non_zero_root() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();

                let mut rng = rand::rngs::StdRng::seed_from_u64(1234);
                let key = $random_key(&mut rng);
                let value = $value_insert;
                storage_transaction.storage_as_mut::<$table>().insert(&key, &value)
                    .unwrap();

                let root = storage_transaction.storage_as_mut::<$table>().root(&key)
                    .expect("Should get the root");
                let empty_root = fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new().root();
                assert_ne!(root, empty_root);
            }

            #[test]
            fn has_different_root_after_each_update() {
                let mut storage = InMemoryStorage::default();
                let mut storage_transaction = storage.write_transaction();

                let mut rng = rand::rngs::StdRng::seed_from_u64(1234);

                let mut prev_root = fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new().root();

                for _ in 0..10 {
                    let key = $random_key(&mut rng);
                    let value = $value_insert;
                    storage_transaction.storage_as_mut::<$table>().insert(&key, &value)
                        .unwrap();

                    let root = storage_transaction.storage_as_mut::<$table>().root(&key)
                        .expect("Should get the root");
                    assert_ne!(root, prev_root);
                    prev_root = root;
                }
            }
        }}
    };
    ($table:ident, $key:expr, $value_insert:expr, $value_return:expr) => {
        $crate::basic_merklelized_storage_tests!(
            $table,
            $key,
            $value_insert,
            $value_return,
            random
        );
    };
    ($table:ident, $key:expr, $value:expr) => {
        $crate::basic_merklelized_storage_tests!($table, $key, $value, $value);
    };
}
