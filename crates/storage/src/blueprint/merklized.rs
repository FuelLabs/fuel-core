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
        Decode,
        Encode,
        Encoder,
    },
    kv_store::{
        BatchOperations,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
    },
    not_found,
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
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
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::binary::Primitive;

#[cfg(feature = "alloc")]
use alloc::borrow::ToOwned;

/// A trait for tables that use the merklized blueprint.
/// Implementing this trait automatically provides a `TableWithBlueprint` implementation
/// that uses the `Merklized` blueprint.
pub trait MerklizedTableWithBlueprint: Mappable + Sized {
    /// The column type used by the merklized table
    type MerkleizedColumn: StorageColumn;

    /// The key codec type for encoding/decoding keys
    type KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>;

    /// The value codec type for encoding/decoding values
    type ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>;

    /// The metadata table type for storing merkle metadata
    type Metadata: Mappable
        + TableWithBlueprint<
            Column = Self::MerkleizedColumn,
            Key = DenseMetadataKey<Self::OwnedKey>,
            OwnedKey = DenseMetadataKey<Self::OwnedKey>,
            Value = DenseMerkleMetadata,
            OwnedValue = DenseMerkleMetadata,
        >;

    /// The nodes table type for storing merkle nodes
    type Nodes: Mappable + TableWithBlueprint<Column = Self::MerkleizedColumn>;

    /// The value encoder type for encoding values for merkle proofs
    type ValueEncoder: Encode<Self::Value>;

    /// The column occupied by the table.
    fn column() -> Self::MerkleizedColumn;
}

/// Automatically implement TableWithBlueprint for any type that implements MerklizedTableWithBlueprint
impl<T> TableWithBlueprint for T
where
    T: MerklizedTableWithBlueprint,
{
    type Blueprint =
        Merklized<T::KeyCodec, T::ValueCodec, T::Metadata, T::Nodes, T::ValueEncoder>;
    type Column = T::MerkleizedColumn;

    fn column() -> Self::Column {
        T::column()
    }
}

/// The `Merklized` blueprint builds the storage as a [`Plain`](super::plain::Plain)
/// blueprint and maintains the binary merkle tree by the `Metadata` table.
///
/// It uses the `KeyCodec` and `ValueCodec` to encode/decode the key and value in the
/// same way as a plain blueprint.
///
/// The `Metadata` table stores the metadata of the binary merkle tree(like a root of the tree and leaves count).
///
/// The `ValueEncoder` is used to encode the value for merklelization.
pub struct Merklized<KeyCodec, ValueCodec, Metadata, Nodes, ValueEncoder> {
    _marker:
        core::marker::PhantomData<(KeyCodec, ValueCodec, Metadata, Nodes, ValueEncoder)>,
}

impl<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
    Merklized<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
where
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
{
    fn insert_into_tree<S, K, V>(
        mut storage: &mut S,
        key: K,
        value: &V,
    ) -> StorageResult<()>
    where
        V: ?Sized,
        Metadata: Mappable<
            Key = DenseMetadataKey<K>,
            Value = DenseMerkleMetadata,
            OwnedValue = DenseMerkleMetadata,
        >,
        S: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        Encoder: Encode<V>,
    {
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

    fn remove<S>(storage: &mut S, key: &[u8], column: S::Column) -> StorageResult<()>
    where
        S: KeyValueMutate,
    {
        if storage.exists(key, column)? {
            Err(anyhow::anyhow!(
                "It is not allowed to remove or override entries in the merklelized table"
            )
            .into())
        } else {
            Ok(())
        }
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, Encoder> BlueprintInspect<M, S>
    for Merklized<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
where
    M: Mappable,
    S: KeyValueInspect,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
{
    type KeyCodec = KeyCodec;
    type ValueCodec = ValueCodec;
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, Encoder> BlueprintMutate<M, S>
    for Merklized<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
where
    M: Mappable,
    S: KeyValueMutate,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Encoder: Encode<M::Value>,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    S: StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
    for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn put(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<()> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let encoded_value = ValueCodec::encode_as_value(value);
        storage.put(key_bytes.as_ref(), column, encoded_value)?;
        let key = key.to_owned().into();
        Self::insert_into_tree(storage, key, value)
    }

    fn replace(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let encoded_value = ValueCodec::encode_as_value(value);
        let prev =
            KeyValueMutate::replace(storage, key_bytes.as_ref(), column, encoded_value)?
                .map(|value| {
                    ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
                })
                .transpose()?;

        if prev.is_some() {
            Self::remove(storage, key_bytes.as_ref(), column)?;
        }

        let key = key.to_owned().into();
        Self::insert_into_tree(storage, key, value)?;
        Ok(prev)
    }

    fn take(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        Self::remove(storage, key_bytes.as_ref(), column)?;
        let prev = KeyValueMutate::take(storage, key_bytes.as_ref(), column)?
            .map(|value| {
                ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()?;
        Ok(prev)
    }

    fn delete(storage: &mut S, key: &M::Key, column: S::Column) -> StorageResult<()> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        Self::remove(storage, key_bytes.as_ref(), column)
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, Encoder> SupportsMerkle<M::Key, M, S>
    for Merklized<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
where
    M: Mappable,
    S: KeyValueInspect,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    Self: BlueprintInspect<M, S>,
    S: StorageInspect<Metadata, Error = StorageError>,
{
    fn root(storage: &S, key: &M::Key) -> StorageResult<MerkleRoot> {
        use crate::StorageAsRef;
        let key = key.to_owned().into();
        let metadata = storage
            .storage_as_ref::<Metadata>()
            .get(&DenseMetadataKey::Primary(key))?
            .ok_or(not_found!(Metadata))?;
        Ok(*metadata.root())
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, Encoder> SupportsBatching<M, S>
    for Merklized<KeyCodec, ValueCodec, Metadata, Nodes, Encoder>
where
    M: Mappable,
    S: BatchOperations,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Encoder: Encode<M::Value>,
    Metadata: Mappable<
        Key = DenseMetadataKey<M::OwnedKey>,
        OwnedKey = DenseMetadataKey<M::OwnedKey>,
        Value = DenseMerkleMetadata,
        OwnedValue = DenseMerkleMetadata,
    >,
    Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
    S: StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
    for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn init<'a, Iter>(storage: &mut S, column: S::Column, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <Self as SupportsBatching<M, S>>::insert(storage, column, set)
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
        for (key, value) in set {
            <Self as BlueprintMutate<M, S>>::replace(storage, key, column, value)?;
        }

        Ok(())
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
        for item in set {
            let key_encoder = KeyCodec::encode(item);
            let key_bytes = key_encoder.as_bytes();
            Self::remove(storage, key_bytes.as_ref(), column)?;
        }
        Ok(())
    }
}

#[cfg(feature = "test-helpers")]
/// A trait that provides basic tests for the merklized storage.
/// It is used to test the merklized storage with different key and value codecs.
#[allow(warnings)]
pub mod basic_tests {
    use core::ops::Deref;

    use crate::{
        blueprint::{
            BlueprintInspect,
            BlueprintMutate,
        },
        codec::Encoder,
        kv_store::KeyValueInspect,
        structured_storage::StructuredStorage,
        transactional::{
            InMemoryTransaction,
            StorageTransaction,
        },
        Error as StorageError,
    };
    use fuel_vm_private::{
        fuel_merkle::binary::Primitive,
        fuel_storage::{
            Mappable,
            StorageAsMut,
            StorageMutate,
        },
    };
    use rand::{
        rngs::StdRng,
        RngCore,
        SeedableRng,
    };

    use crate::{
        blueprint::merklized::Merklized,
        codec::{
            Decode,
            Encode,
        },
        structured_storage::{
            test::InMemoryStorage,
            TableWithBlueprint,
        },
        tables::merkle::{
            DenseMerkleMetadata,
            DenseMetadataKey,
        },
        transactional::WriteTransaction,
    };

    use crate::blueprint::merklized::MerklizedTableWithBlueprint;

    #[allow(dead_code)]
    /// A trait that provides basic tests for the merklized storage.
    /// It is used to test the merklized storage with different key and value codecs.
    pub trait BasicMerkleizedStorageTests: MerklizedTableWithBlueprint
    where
        Self::KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>,
        Self::ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>,
        Self::ValueEncoder: Encode<Self::Value>,
        Self::Metadata: TableWithBlueprint<Column = Self::MerkleizedColumn>,
        Self::Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>,
        Self::Nodes: TableWithBlueprint<Column = Self::MerkleizedColumn>,
        Self::OwnedValue: PartialEq + core::fmt::Debug,
        Self::MerkleizedColumn: PartialEq,

        for<'a, 'b> <Self::Metadata as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Self::Metadata,
            StructuredStorage<
                &'a mut StorageTransaction<
                    &'b mut InMemoryStorage<Self::MerkleizedColumn>,
                >,
            >,
        >,
        for<'a> <Self::Metadata as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Self::Metadata,
            StorageTransaction<&'a mut InMemoryStorage<Self::MerkleizedColumn>>,
        >,

        for<'a, 'b> <Self::Nodes as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Self::Nodes,
            StructuredStorage<
                &'a mut StorageTransaction<
                    &'b mut InMemoryStorage<Self::MerkleizedColumn>,
                >,
            >,
        >,
        for<'a> <Self::Nodes as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Self::Nodes,
            StorageTransaction<&'a mut InMemoryStorage<Self::MerkleizedColumn>>,
        >,
    {
        /// Returns a test key for the table
        fn key() -> Box<Self::Key>;

        /// Returns a random key for testing
        fn random_key(rng: &mut StdRng) -> Box<Self::Key>;

        /// Returns a test value for the table
        fn value() -> Box<Self::Value>;

        /// Tests that getting a value returns the same value that was inserted
        fn test_get() {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();
            let key = Self::key();

            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&key, &Self::value())
                .unwrap();

            assert_eq!(
                storage_transaction
                    .storage_as_mut::<Self>()
                    .get(&key)
                    .expect("Should get without errors")
                    .expect("Should not be empty")
                    .into_owned(),
                Self::value().to_owned().into()
            );
        }

        /// Tests that inserting a value and retrieving it returns the same value
        fn test_insert() {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();
            let key = Self::key();

            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&key, &Self::value())
                .unwrap();

            let returned = storage_transaction
                .storage_as_mut::<Self>()
                .get(&key)
                .unwrap()
                .unwrap()
                .into_owned();

            assert_eq!(returned, Self::value().to_owned().into());
        }

        /// Tests that attempting to remove a value returns an error
        fn test_remove_returns_error() {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&Self::key(), &Self::value())
                .unwrap();

            let result = storage_transaction
                .storage_as_mut::<Self>()
                .remove(&Self::key());

            assert!(result.is_err());
        }

        /// Tests that checking for key existence works correctly
        fn test_exists() {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();
            let key = Self::key();

            // Given
            assert!(!storage_transaction
                .storage_as_mut::<Self>()
                .contains_key(&key)
                .unwrap());

            // When
            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&key, &Self::value())
                .unwrap();

            // Then
            assert!(storage_transaction
                .storage_as_mut::<Self>()
                .contains_key(&key)
                .unwrap());
        }

        /// Tests that batch mutation operations work correctly
        fn test_batch_mutate_works() {
            let empty_storage = InMemoryStorage::default();

            let mut init_storage = InMemoryStorage::default();
            let mut init_structured_storage = init_storage.write_transaction();

            let mut rng = &mut StdRng::seed_from_u64(31337);
            let gen = || Some(Self::random_key(&mut rng));
            let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
            let value = Self::value();

            <_ as crate::StorageBatchMutate<Self>>::init_storage(
                &mut init_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &<Self as crate::Mappable>::Value = &value;
                    (k.as_ref(), value)
                }),
            )
            .expect("Should initialize the storage successfully");
            init_structured_storage
                .commit()
                .expect("Should commit the storage");

            let mut insert_storage = InMemoryStorage::default();
            let mut insert_structured_storage = insert_storage.write_transaction();

            <_ as crate::StorageBatchMutate<Self>>::insert_batch(
                &mut insert_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &<Self as crate::Mappable>::Value = &value;
                    (k.as_ref(), value)
                }),
            )
            .expect("Should insert batch successfully");
            insert_structured_storage
                .commit()
                .expect("Should commit the storage");

            assert_eq!(init_storage, insert_storage);
            assert_ne!(init_storage, empty_storage);
            assert_ne!(insert_storage, empty_storage);
        }

        /// Tests that batch removal operations fail
        fn test_batch_remove_fails() {
            let mut init_storage = InMemoryStorage::default();
            let mut init_structured_storage = init_storage.write_transaction();

            let mut rng = &mut StdRng::seed_from_u64(31337);
            let gen = || Some(Self::random_key(&mut rng));
            let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
            let value = Self::value();

            <_ as crate::StorageBatchMutate<Self>>::init_storage(
                &mut init_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &Self::Value = &value;
                    (k.as_ref(), value)
                }),
            )
            .expect("Should initialize the storage successfully");

            let result = <_ as crate::StorageBatchMutate<Self>>::remove_batch(
                &mut init_structured_storage,
                &mut data.iter().map(|key| key.deref()),
            );

            assert!(result.is_err());
        }

        /// Tests that getting the root fails when there's no metadata
        fn test_root_returns_error_empty_metadata()
        where
            Self::Key: Sized,
        {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            let root = storage_transaction
                .storage_as_mut::<Self>()
                .root(&Self::key());
            assert!(root.is_err())
        }

        /// Tests that updating produces a non-zero root
        fn test_update_produces_non_zero_root()
        where
            Self::Key: Sized,
        {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            let mut rng = &mut StdRng::seed_from_u64(1234);
            let key = Self::random_key(&mut rng);
            let value = Self::value();
            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&key, &value)
                .unwrap();

            let root = storage_transaction
                .storage_as_mut::<Self>()
                .root(&key)
                .expect("Should get the root");
            let empty_root =
                fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new().root();
            assert_ne!(root, empty_root);
        }

        /// Tests that each update produces a different root
        fn test_has_different_root_after_each_update()
        where
            Self::Key: Sized,
        {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            let mut rng = &mut StdRng::seed_from_u64(1234);

            let mut prev_root =
                fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new().root();

            for _ in 0..10 {
                let key = Self::random_key(&mut rng);
                let value = Self::value();
                storage_transaction
                    .storage_as_mut::<Self>()
                    .insert(&key, &value)
                    .unwrap();

                let root = storage_transaction
                    .storage_as_mut::<Self>()
                    .root(&key)
                    .expect("Should get the root");
                assert_ne!(root, prev_root);
                prev_root = root;
            }
        }

        /// Tests that we can generate and validate merkle proofs
        fn test_can_generate_and_validate_proofs()
        where
            Self::Key: Sized,
            Self::OwnedKey: Into<u32>,
        {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            let mut rng = &mut StdRng::seed_from_u64(1234);
            let key = Self::random_key(&mut rng);
            let owned_key = Self::OwnedKey::from(key.to_owned());

            let value = Self::value();

            let encoded_value = Self::ValueEncoder::encode(&value);

            storage_transaction
                .storage_as_mut::<Self>()
                .insert(&key, &value)
                .unwrap();

            let root = storage_transaction
                .storage_as_mut::<Self>()
                .root(&key)
                .expect("Should get the root");

            let merkle_metadata = storage_transaction
                .storage::<Self::Metadata>()
                .get(&DenseMetadataKey::Primary(owned_key))
                .expect("expected metadata")
                .unwrap();

            let num_leaves = merkle_metadata.version();
            let proof_index = num_leaves.checked_sub(1).unwrap();

            let tree: fuel_core_types::fuel_merkle::binary::MerkleTree<Self::Nodes, _> =
                fuel_core_types::fuel_merkle::binary::MerkleTree::load(
                    &storage_transaction,
                    num_leaves,
                )
                .expect("could not load merkle tree");

            let (returned_root, returned_proof_set) =
                tree.prove(proof_index).expect("failed to produce proof");

            let proof_is_valid = fuel_core_types::fuel_merkle::binary::verify(
                &returned_root,
                &encoded_value.as_bytes(),
                &returned_proof_set,
                proof_index,
                num_leaves,
            );
            assert!(proof_is_valid);

            assert_eq!(returned_root, root);
        }
    }
}

/// The macro that generates basic storage tests for the table with the merklelized structure.
/// It uses the [`InMemoryStorage`](crate::structured_storage::test::InMemoryStorage).
#[cfg(feature = "test-helpers")]
#[macro_export]
macro_rules! basic_merklelized_storage_tests {
    ($table:ident, $key:expr, $value:expr, $random_key:expr) => {
        $crate::paste::item! {
            #[cfg(test)]
            mod [< $table:snake _basic_tests >] {
                use super::*;
                use $crate::blueprint::merklized::basic_tests::BasicMerkleizedStorageTests;

                impl BasicMerkleizedStorageTests for $table {
                    fn key() -> Box<Self::Key> {
                        Box::new($key)
                    }

                    fn random_key(rng: &mut rand::rngs::StdRng) -> Box<Self::Key> {
                        Box::new($random_key(rng))
                    }

                    fn value() -> Box<Self::Value> {
                        Box::new($value)
                    }
                }

                #[test]
                fn merkleized_storage__test_get() {
                    $table::test_get();
                }

                #[test]
                fn merkleized_storage__test_insert() {
                    $table::test_insert();
                }

                #[test]
                fn merkleized_storage__test_remove_returns_error() {
                    $table::test_remove_returns_error();
                }

                #[test]
                fn merkleized_storage__test_exists() {
                    $table::test_exists();
                }

                #[test]
                fn merkleized_storage__test_batch_mutate_works() {
                    $table::test_batch_mutate_works();
                }

                #[test]
                fn merkleized_storage__test_batch_remove_fails() {
                    $table::test_batch_remove_fails();
                }

                #[test]
                fn merkleized_storage__test_root_returns_error_empty_metadata() {
                    $table::test_root_returns_error_empty_metadata();
                }

                #[test]
                fn merkleized_storage__test_update_produces_non_zero_root() {
                    $table::test_update_produces_non_zero_root();
                }

                #[test]
                fn merkleized_storage__test_has_different_root_after_each_update() {
                    $table::test_has_different_root_after_each_update();
                }

                #[test]
                fn merkleized_storage__test_can_generate_and_validate_proofs() {
                    $table::test_can_generate_and_validate_proofs();
                }
            }
        }
    };
}
