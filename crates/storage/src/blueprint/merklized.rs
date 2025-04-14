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
    },
    not_found,
    structured_storage::StructuredStorage,
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

/// New test infrastructure that will eventually replace the above
#[cfg(feature = "test-helpers")]
pub mod basic_tests_bmt {
    use super::*;
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
        transactional::{
            StorageTransaction,
            WriteTransaction,
        },
    };
    use core::fmt;
    use fuel_core_types::fuel_merkle::binary::{
        self,
        MerkleTree,
        Primitive,
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    /// A wrapper type to allow for `AsRef` implementation.
    pub struct Wrapper<T>(pub T);

    impl<T> AsRef<T> for Wrapper<T> {
        fn as_ref(&self) -> &T {
            &self.0
        }
    }

    /// The trait that generates test data for the BMT storage table.
    pub trait BMTTestDataGenerator {
        /// The key type of the table.
        type Key;
        /// The value type of the table.
        type Value;

        /// Returns a test key for the table
        fn key() -> Self::Key;

        /// Returns a random key for testing
        fn random_key(rng: &mut StdRng) -> Self::Key;

        /// Generates a random value
        fn generate_value(rng: &mut StdRng) -> Self::Value;
    }

    /// Provides test implementations for BMT storage table.
    pub struct BmtTests<M>(core::marker::PhantomData<M>);

    impl<M, Key, Value, Metadata, Nodes> BmtTests<M>
    where
        M: MerklizedTableWithBlueprint<Key = Key, Metadata = Metadata, Nodes = Nodes>
            + BMTTestDataGenerator<Key = Key, Value = Value>,
        M::Column: PartialEq,
        Metadata: Mappable<
                Key = DenseMetadataKey<M::OwnedKey>,
                OwnedKey = DenseMetadataKey<M::OwnedKey>,
                Value = DenseMerkleMetadata,
                OwnedValue = DenseMerkleMetadata,
            > + TableWithBlueprint<Column = M::Column>,
        Nodes: Mappable<Key = u64, Value = Primitive, OwnedValue = Primitive>
            + TableWithBlueprint<Column = M::Column>,
        Key: Sized + ToOwned,
        Value: Sized + AsRef<<M as Mappable>::Value>,
        M::OwnedValue: PartialEq + fmt::Debug,
        for<'a> StorageTransaction<&'a mut InMemoryStorage<M::Column>>: StorageMutate<M, Error = StorageError>
            + StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        for<'a, 'b> <Metadata as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Metadata,
            StructuredStorage<
                &'a mut StorageTransaction<&'b mut InMemoryStorage<M::Column>>,
            >,
        >,
        for<'a, 'b> <Nodes as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Nodes,
            StructuredStorage<
                &'a mut StorageTransaction<&'b mut InMemoryStorage<M::Column>>,
            >,
        >,
        for<'a> <Metadata as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Metadata,
            StorageTransaction<&'a mut InMemoryStorage<M::Column>>,
        >,
        for<'a> <Nodes as TableWithBlueprint>::Blueprint: BlueprintMutate<
            Nodes,
            StorageTransaction<&'a mut InMemoryStorage<M::Column>>,
        >,
    {
        /// Tests that getting a value returns the same value that was inserted
        pub fn test_get() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let key = M::key();
            let value = M::generate_value(&mut StdRng::seed_from_u64(1234));

            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let result = storage_transaction
                .storage_as_mut::<M>()
                .get(&key)
                .expect("Should get without errors")
                .expect("Should not be empty")
                .into_owned();

            assert_eq!(result, value.as_ref().to_owned().into());
        }

        /// Tests that inserting a value and retrieving it returns the same value
        pub fn test_insert() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let key = M::key();
            let value = M::generate_value(&mut StdRng::seed_from_u64(1234));

            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let returned = storage_transaction
                .storage_as_mut::<M>()
                .get(&key)
                .unwrap()
                .unwrap()
                .into_owned();

            assert_eq!(returned, value.as_ref().to_owned().into());
        }

        /// Tests that attempting to remove a value returns an error
        pub fn test_remove_returns_error() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let key = M::key();
            let value = M::generate_value(&mut StdRng::seed_from_u64(1234));

            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let result = storage_transaction.storage_as_mut::<M>().remove(&key);

            assert!(result.is_err());
        }

        /// Tests that checking for key existence works correctly
        pub fn test_exists() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();
            let key = M::key();
            let value = M::generate_value(&mut StdRng::seed_from_u64(1234));

            // Given
            assert!(!storage_transaction
                .storage_as_mut::<M>()
                .contains_key(&key)
                .unwrap());

            // When
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            // Then
            assert!(storage_transaction
                .storage_as_mut::<M>()
                .contains_key(&key)
                .unwrap());
        }

        /// Tests that batch mutation operations work correctly
        pub fn test_batch_mutate_works() {
            let empty_storage = InMemoryStorage::<M::Column>::default();

            let mut init_storage = InMemoryStorage::<M::Column>::default();
            let mut init_structured_storage = init_storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(31337);
            let gen = || Some(M::random_key(rng));
            let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
            let value = M::generate_value(rng);

            <_ as crate::StorageBatchMutate<M>>::init_storage(
                &mut init_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &<M as crate::Mappable>::Value = value.as_ref();
                    (k, value)
                }),
            )
            .expect("Should initialize the storage successfully");
            init_structured_storage
                .commit()
                .expect("Should commit the storage");

            let mut insert_storage = InMemoryStorage::<M::Column>::default();
            let mut insert_structured_storage = insert_storage.write_transaction();

            <_ as crate::StorageBatchMutate<M>>::insert_batch(
                &mut insert_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &<M as crate::Mappable>::Value = value.as_ref();
                    (k, value)
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
        pub fn test_batch_remove_fails() {
            let mut init_storage = InMemoryStorage::<M::Column>::default();
            let mut init_structured_storage = init_storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(31337);
            let gen = || Some(M::random_key(rng));
            let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
            let value = M::generate_value(rng);

            <_ as crate::StorageBatchMutate<M>>::init_storage(
                &mut init_structured_storage,
                &mut data.iter().map(|k| {
                    let value: &<M as crate::Mappable>::Value = value.as_ref();
                    (k, value)
                }),
            )
            .expect("Should initialize the storage successfully");

            let result = <_ as crate::StorageBatchMutate<M>>::remove_batch(
                &mut init_structured_storage,
                &mut data.iter(),
            );

            assert!(result.is_err());
        }

        /// Tests that getting the root fails when there's no metadata
        pub fn test_root_returns_error_empty_metadata() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let root = storage_transaction.storage_as_mut::<M>().root(&M::key());
            assert!(root.is_err())
        }

        /// Tests that updating produces a non-zero root
        pub fn test_update_produces_non_zero_root() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let key = M::random_key(rng);
            let value = M::generate_value(rng);

            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let root = storage_transaction
                .storage_as_mut::<M>()
                .root(&key)
                .expect("Should get the root");

            let empty_root = binary::in_memory::MerkleTree::new().root();
            assert_ne!(root, empty_root);
        }

        /// Tests that each update produces a different root
        pub fn test_has_different_root_after_each_update() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let mut prev_root = binary::in_memory::MerkleTree::new().root();

            for _ in 0..10 {
                let key = M::random_key(rng);
                let value = M::generate_value(rng);
                storage_transaction
                    .storage_as_mut::<M>()
                    .insert(&key, value.as_ref())
                    .unwrap();

                let root = storage_transaction
                    .storage_as_mut::<M>()
                    .root(&key)
                    .expect("Should get the root");
                assert_ne!(root, prev_root);
                prev_root = root;
            }
        }

        /// Tests that we can generate and validate merkle proofs
        pub fn test_can_generate_and_validate_proofs() {
            let mut storage = InMemoryStorage::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let key = M::random_key(rng);

            let key: <M as Mappable>::Key = key;
            let owned_key = key.to_owned().into();

            let value = M::generate_value(rng);

            let encoded_value = M::ValueEncoder::encode(value.as_ref());

            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let root = storage_transaction
                .storage_as_mut::<M>()
                .root(&key)
                .expect("Should get the root");

            let merkle_metadata = storage_transaction
                .storage::<M::Metadata>()
                .get(&DenseMetadataKey::Primary(owned_key))
                .expect("expected metadata")
                .unwrap();

            let num_leaves = merkle_metadata.version();
            let proof_index = num_leaves.checked_sub(1).unwrap();

            let tree: MerkleTree<M::Nodes, _> =
                MerkleTree::load(&storage_transaction, num_leaves)
                    .expect("could not load merkle tree");

            let (returned_root, returned_proof_set) =
                tree.prove(proof_index).expect("failed to produce proof");

            let proof_is_valid = binary::verify(
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

    /// Helper trait enabling referencing generics in
    /// Merklized `TableWithBlueprint` implementations
    /// as associated types.
    pub trait MerklizedTableWithBlueprint:
        TableWithBlueprint<
        Blueprint = Merklized<
            Self::KeyCodec,
            Self::ValueCodec,
            Self::Metadata,
            Self::Nodes,
            Self::ValueEncoder,
        >,
    >
    {
        /// The key codec type for encoding/decoding keys
        type KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>;

        /// The value codec type for encoding/decoding values
        type ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>;

        /// The metadata table type for storing merkle metadata
        type Metadata: TableWithBlueprint<
            Column = Self::Column,
            Key = DenseMetadataKey<Self::OwnedKey>,
            OwnedKey = DenseMetadataKey<Self::OwnedKey>,
            Value = DenseMerkleMetadata,
            OwnedValue = DenseMerkleMetadata,
        >;

        /// The nodes table type for storing merkle nodes
        type Nodes: TableWithBlueprint<
            Key = u64,
            Value = Primitive,
            OwnedValue = Primitive,
            Column = Self::Column,
        >;

        /// The value encoder type for encoding values for merkle proofs
        type ValueEncoder: Encode<Self::Value>;
    }

    impl<T, KeyCodec, ValueCodec, Metadata, Nodes, ValueEncoder>
        MerklizedTableWithBlueprint for T
    where
        T: TableWithBlueprint<
            Blueprint = Merklized<KeyCodec, ValueCodec, Metadata, Nodes, ValueEncoder>,
        >,
        KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>,
        ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>,
        Metadata: TableWithBlueprint<
            Column = Self::Column,
            Key = DenseMetadataKey<Self::OwnedKey>,
            OwnedKey = DenseMetadataKey<Self::OwnedKey>,
            Value = DenseMerkleMetadata,
            OwnedValue = DenseMerkleMetadata,
        >,
        Nodes: TableWithBlueprint<
            Key = u64,
            Value = Primitive,
            OwnedValue = Primitive,
            Column = Self::Column,
        >,
        ValueEncoder: Encode<Self::Value>,
    {
        type KeyCodec = KeyCodec;
        type ValueCodec = ValueCodec;
        type Metadata = Metadata;
        type Nodes = Nodes;
        type ValueEncoder = ValueEncoder;
    }

    /// Generates test functions for tables using the binary merkle tree structure.
    #[macro_export]
    macro_rules! basic_merklized_storage_tests {
        ($table:ident) => {
            #[test]
            fn merkleized_storage__test_get() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_get();
            }

            #[test]
            fn merkleized_storage__test_insert() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_insert();
            }

            #[test]
            fn merkleized_storage__test_remove_returns_error() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_remove_returns_error();
            }

            #[test]
            fn merkleized_storage__test_exists() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_exists();
            }

            #[test]
            fn merkleized_storage__test_batch_mutate_works() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_batch_mutate_works();
            }

            #[test]
            fn merkleized_storage__test_batch_remove_fails() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_batch_remove_fails();
            }

            #[test]
            fn merkleized_storage__test_root_returns_error_empty_metadata() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_root_returns_error_empty_metadata();
            }

            #[test]
            fn merkleized_storage__test_update_produces_non_zero_root() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_update_produces_non_zero_root();
            }

            #[test]
            fn merkleized_storage__test_has_different_root_after_each_update() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_has_different_root_after_each_update();
            }

            #[test]
            fn merkleized_storage__test_can_generate_and_validate_proofs() {
                $crate::blueprint::merklized::basic_tests_bmt::BmtTests::<$table>::test_can_generate_and_validate_proofs();
            }
        };
    }
}
