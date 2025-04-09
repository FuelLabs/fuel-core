//! The module defines the `Sparse` blueprint for the storage.
//! The `Sparse` blueprint implements the sparse merkle tree on top of the storage.
//! It is like a [`Plain`](super::plain::Plain) blueprint that builds the sparse
//! merkle tree parallel to the normal storage and maintains it.

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
        WriteOperation,
    },
    structured_storage::TableWithBlueprint,
    tables::merkle::SparseMerkleMetadata,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result as StorageResult,
    StorageAsMut,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::{
    sparse,
    sparse::{
        in_memory,
        MerkleTree,
        MerkleTreeKey,
    },
};
use itertools::Itertools;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

/// The trait that allows to convert the key of the table into the key of the metadata table.
/// If the key comprises several entities, it is possible to build a Merkle tree over different primary keys.
/// The trait defines the key over which to build an SMT.
pub trait PrimaryKey {
    /// The storage key of the table.
    type InputKey: ?Sized;
    /// The extracted primary key.
    type OutputKey: ?Sized + Copy;

    /// Converts the key of the table into the primary key of the metadata table.
    fn primary_key(key: &Self::InputKey) -> Cow<'_, Self::OutputKey>;
}

/// The `Sparse` blueprint builds the storage as a [`Plain`](super::plain::Plain)
/// blueprint and maintains the sparse merkle tree by the `Metadata` and `Nodes` tables.
///
/// It uses the `KeyCodec` and `ValueCodec` to encode/decode the key and value in the
/// same way as a plain blueprint.
///
/// The `Metadata` table stores the metadata of the tree(like a root of the tree),
/// and the `Nodes` table stores the tree's nodes. The SMT is built over the encoded
/// keys and values using the same encoding as for main key-value pairs.
///
/// The `KeyConverter` is used to convert the key of the table into the primary key of the metadata table.
pub struct Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> {
    _marker:
        core::marker::PhantomData<(KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter)>,
}

impl<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
    Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
        Key = MerkleRoot,
        Value = sparse::Primitive,
        OwnedValue = sparse::Primitive,
    >,
{
    fn insert_into_tree<S, K>(
        storage: &mut S,
        key: &K,
        key_bytes: &[u8],
        value_bytes: &[u8],
    ) -> StorageResult<()>
    where
        K: ?Sized,
        S: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        KeyConverter: PrimaryKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let primary_key = KeyConverter::primary_key(key);
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key.as_ref())?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        tree.update(MerkleTreeKey::new(key_bytes), value_bytes)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Generate new metadata for the updated tree
        let root = tree.root();
        let storage = tree.into_storage();
        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key.as_ref(), &metadata)?;
        Ok(())
    }

    fn remove_from_tree<S, K>(
        storage: &mut S,
        key: &K,
        key_bytes: &[u8],
    ) -> StorageResult<()>
    where
        K: ?Sized,
        S: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        KeyConverter: PrimaryKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let primary_key = KeyConverter::primary_key(key);
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Option<Cow<SparseMerkleMetadata>> =
            storage.storage::<Metadata>().get(primary_key.as_ref())?;

        if let Some(prev_metadata) = prev_metadata {
            let root = *prev_metadata.root();

            let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(storage, &root)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root = tree.root();
            let storage = tree.into_storage();
            if &root == MerkleTree::<Nodes, S>::empty_root() {
                // The tree is now empty; remove the metadata
                storage.storage::<Metadata>().remove(primary_key.as_ref())?;
            } else {
                // Generate new metadata for the updated tree
                let metadata = SparseMerkleMetadata::new(root);
                storage
                    .storage::<Metadata>()
                    .insert(primary_key.as_ref(), &metadata)?;
            }
        }

        Ok(())
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> BlueprintInspect<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    M: Mappable,
    S: KeyValueInspect,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
{
    type KeyCodec = KeyCodec;
    type ValueCodec = ValueCodec;
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> BlueprintMutate<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    M: Mappable,
    S: KeyValueMutate,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
        Key = MerkleRoot,
        Value = sparse::Primitive,
        OwnedValue = sparse::Primitive,
    >,
    KeyConverter: PrimaryKey<InputKey = M::Key, OutputKey = Metadata::Key>,
    S: StorageMutate<Metadata, Error = StorageError>
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
        let value = ValueCodec::encode_as_value(value);
        storage.put(key_bytes.as_ref(), column, value.clone())?;
        Self::insert_into_tree(storage, key, key_bytes.as_ref(), value.as_ref())
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
        let prev =
            KeyValueMutate::replace(storage, key_bytes.as_ref(), column, value.clone())?
                .map(|value| {
                    ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
                })
                .transpose()?;

        Self::insert_into_tree(storage, key, key_bytes.as_ref(), value.as_ref())?;
        Ok(prev)
    }

    fn take(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let prev = KeyValueMutate::take(storage, key_bytes.as_ref(), column)?
            .map(|value| {
                ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()?;
        Self::remove_from_tree(storage, key, key_bytes.as_ref())?;
        Ok(prev)
    }

    fn delete(storage: &mut S, key: &M::Key, column: S::Column) -> StorageResult<()> {
        let key_encoder = KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.delete(key_bytes.as_ref(), column)?;
        Self::remove_from_tree(storage, key, key_bytes.as_ref())
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
    SupportsMerkle<Metadata::Key, M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    M: Mappable,
    S: KeyValueInspect,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Self: BlueprintInspect<M, S>,
    S: StorageInspect<Metadata, Error = StorageError>,
{
    fn root(storage: &S, key: &Metadata::Key) -> StorageResult<MerkleRoot> {
        use crate::StorageAsRef;
        let metadata: Option<Cow<SparseMerkleMetadata>> =
            storage.storage_as_ref::<Metadata>().get(key)?;
        let root = metadata
            .map(|metadata| *metadata.root())
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        Ok(root)
    }
}

type NodeKeyCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as BlueprintInspect<Nodes, S>>::KeyCodec;
type NodeValueCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as BlueprintInspect<Nodes, S>>::ValueCodec;

impl<Column, M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
    SupportsBatching<M, S> for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    Column: StorageColumn,
    S: BatchOperations<Column = Column>,
    M: TableWithBlueprint<
        Blueprint = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>,
        Column = Column,
    >,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
            Key = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
        > + TableWithBlueprint<Column = Column>,
    KeyConverter: PrimaryKey<InputKey = M::Key, OutputKey = Metadata::Key>,
    Nodes::Blueprint: BlueprintInspect<Nodes, S>,
    S: StorageMutate<M, Error = StorageError>
        + StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn init<'a, Iter>(storage: &mut S, column: S::Column, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        let mut set = set.peekable();

        let primary_key;
        if let Some((key, _)) = set.peek() {
            primary_key = KeyConverter::primary_key(*key);
        } else {
            return Ok(())
        }

        if storage
            .storage::<Metadata>()
            .contains_key(primary_key.as_ref())?
        {
            return Err(anyhow::anyhow!(
                "The {} is already initialized",
                M::column().name()
            )
            .into())
        }

        let encoded_set = set
            .map(|(key, value)| {
                let key = KeyCodec::encode(key).as_bytes().into_owned();
                let value = ValueCodec::encode(value).as_bytes().into_owned();
                (key, value)
            })
            .collect_vec();

        let (root, nodes) = in_memory::MerkleTree::nodes_from_set(
            encoded_set
                .iter()
                .map(|(key, value)| (MerkleTreeKey::new(key), value)),
        );

        storage.batch_write(
            column,
            encoded_set
                .into_iter()
                .map(|(key, value)| (key, WriteOperation::Insert(value.into()))),
        )?;

        let nodes = nodes.iter().map(|(key, value)| {
            let key = NodeKeyCodec::<S, Nodes>::encode(key)
                .as_bytes()
                .into_owned();
            let value = NodeValueCodec::<S, Nodes>::encode_as_value(value);
            (key, WriteOperation::Insert(value))
        });
        storage.batch_write(Nodes::column(), nodes)?;

        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key.as_ref(), &metadata)?;

        Ok(())
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
        let mut set = set.peekable();

        let primary_key;
        if let Some((key, _)) = set.peek() {
            primary_key = KeyConverter::primary_key(*key);
        } else {
            return Ok(())
        }

        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key.as_ref())?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        let encoded_set = set
            .map(|(key, value)| {
                let key = KeyCodec::encode(key).as_bytes().into_owned();
                let value = ValueCodec::encode(value).as_bytes().into_owned();
                (key, value)
            })
            .collect_vec();

        for (key_bytes, value_bytes) in encoded_set.iter() {
            tree.update(MerkleTreeKey::new(key_bytes), value_bytes)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        }
        let root = tree.root();
        let storage = tree.into_storage();

        storage.batch_write(
            column,
            encoded_set
                .into_iter()
                .map(|(key, value)| (key, WriteOperation::Insert(value.into()))),
        )?;

        // Generate new metadata for the updated tree
        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key.as_ref(), &metadata)?;

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
        let mut set = set.peekable();

        let primary_key;
        if let Some(key) = set.peek() {
            primary_key = KeyConverter::primary_key(*key);
        } else {
            return Ok(())
        }

        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key.as_ref())?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        let encoded_set = set
            .map(|key| KeyCodec::encode(key).as_bytes().into_owned())
            .collect_vec();

        for key_bytes in encoded_set.iter() {
            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        }
        let root = tree.root();
        let storage = tree.into_storage();

        storage.batch_write(
            column,
            encoded_set
                .into_iter()
                .map(|key| (key, WriteOperation::Remove)),
        )?;

        if &root == MerkleTree::<Nodes, S>::empty_root() {
            // The tree is now empty; remove the metadata
            storage.storage::<Metadata>().remove(primary_key.as_ref())?;
        } else {
            // Generate new metadata for the updated tree
            let metadata = SparseMerkleMetadata::new(root);
            storage
                .storage::<Metadata>()
                .insert(primary_key.as_ref(), &metadata)?;
        }

        Ok(())
    }
}

#[cfg(feature = "test-helpers")]
/// Test module for root storage tests.
pub mod root_storage_tests_smt {
    use core::fmt;
    use fuel_core_types::fuel_merkle::storage::StorageMutate;
    use fuel_vm_private::{
        fuel_merkle::sparse::{
            self,
            proof::Proof,
            MerkleTree,
            MerkleTreeKey,
        },
        fuel_storage::{
            Mappable,
            StorageAsMut,
        },
    };
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    use crate::{
        blueprint::sparse::{
            PrimaryKey,
            Sparse,
        },
        codec::{
            Decode,
            Encode,
            Encoder,
        },
        structured_storage::{
            test::InMemoryStorage,
            TableWithBlueprint,
        },
        tables::merkle::SparseMerkleMetadata,
        transactional::{
            StorageTransaction,
            WriteTransaction,
        },
        MerkleRoot,
        MerkleRootStorage,
    };

    /// A wrapper type to allow for `AsRef` implementation.
    pub struct Wrapper<T>(pub T);

    impl<T> AsRef<T> for Wrapper<T> {
        fn as_ref(&self) -> &T {
            &self.0
        }
    }

    /// The trait that generates test data for the SMT storage table.
    pub trait SMTTestDataGenerator {
        /// The key type of the table.
        type Key;
        /// The primary key type of the table.
        type PrimaryKey;
        /// The value type of the table.
        type Value;

        /// Returns a test primary key
        fn primary_key() -> Self::PrimaryKey;

        /// Returns a different primary key for testing isolation
        fn foreign_key() -> Self::PrimaryKey;

        /// Generates a random key for the given primary key
        fn generate_key(current_key: &Self::PrimaryKey, rng: &mut StdRng) -> Self::Key;

        /// Generates a random value
        fn generate_value(rng: &mut StdRng) -> Self::Value;
    }

    /// Provides root storage tests for SMT storage table.
    pub struct SmtTests<M>(core::marker::PhantomData<M>);

    impl<M, Key, PrimaryKey, Value, Metadata, Nodes, Error> SmtTests<M>
    where
        M: SmtTableWithBlueprint<Key = Key, Metadata = Metadata, Nodes = Nodes>
            + SMTTestDataGenerator<Key = Key, PrimaryKey = PrimaryKey, Value = Value>,
        Metadata: Mappable<
            Key = PrimaryKey,
            OwnedKey = PrimaryKey,
            Value = SparseMerkleMetadata,
            OwnedValue = SparseMerkleMetadata,
        >,
        Nodes: Mappable<
            Key = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
        >,
        PrimaryKey: Sized,
        Key: Sized,
        Value: Sized + AsRef<<M as Mappable>::Value>,
        Error: fmt::Debug,
        for<'a> StorageTransaction<&'a mut InMemoryStorage<M::Column>>: StorageMutate<M, Error = Error>
            + StorageMutate<Metadata, Error = Error>
            + StorageMutate<Nodes, Error = Error>
            + MerkleRootStorage<PrimaryKey, M, Error = Error>,
    {
        /// Tests that getting a root after insertion works
        pub fn test_root() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let current_key = M::primary_key();
            let key = M::generate_key(&current_key, rng);

            let value = M::generate_value(rng);
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, value.as_ref())
                .unwrap();

            let root = storage_transaction.storage_as_mut::<M>().root(&current_key);
            assert!(root.is_ok())
        }

        /// Tests that an empty tree returns the expected empty root
        pub fn test_root_returns_empty_root_for_empty_metadata() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let empty_root = sparse::in_memory::MerkleTree::new().root();
            let current_key = M::primary_key();
            let root = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();
            assert_eq!(root, empty_root)
        }

        /// Tests that inserting different states produces different merkle roots
        pub fn test_put_updates_the_state_merkle_root_for_the_given_metadata() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let current_key = M::primary_key();
            let key = M::generate_key(&current_key, rng);
            let state = M::generate_value(rng);

            // Write the first state
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, state.as_ref())
                .unwrap();

            // Read the first Merkle root
            let root_1 = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            // Write the second state
            let key = M::generate_key(&current_key, rng);
            let state = M::generate_value(rng);
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, state.as_ref())
                .unwrap();

            // Read the second Merkle root
            let root_2 = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            assert_ne!(root_1, root_2);
        }

        /// Tests that removing a state updates the merkle root and returns it to the previous state
        pub fn test_remove_updates_the_state_merkle_root_for_the_given_metadata() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let current_key = M::primary_key();

            // Write the first state
            let first_key = M::generate_key(&current_key, rng);
            let first_state = M::generate_value(rng);
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&first_key, first_state.as_ref())
                .unwrap();
            let root_0 = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            // Write the second state
            let second_key = M::generate_key(&current_key, rng);
            let second_state = M::generate_value(rng);
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&second_key, second_state.as_ref())
                .unwrap();

            // Read the first Merkle root
            let root_1 = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            // Remove the second state
            storage_transaction
                .storage_as_mut::<M>()
                .remove(&second_key)
                .unwrap();

            // Read the second Merkle root
            let root_2 = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            assert_ne!(root_1, root_2);
            assert_eq!(root_0, root_2);
        }

        /// Tests that operations on one metadata key don't affect another
        pub fn test_updating_foreign_metadata_does_not_affect_the_given_metadata_insertion(
        ) {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let state_value = M::generate_value(rng);

            // Given
            let given_key = M::generate_key(&M::primary_key(), rng);
            let foreign_key = M::generate_key(&M::foreign_key(), rng);
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&given_key, state_value.as_ref())
                .unwrap();

            // When
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&foreign_key, state_value.as_ref())
                .unwrap();
            storage_transaction
                .storage_as_mut::<M>()
                .remove(&foreign_key)
                .unwrap();

            // Then
            let result = storage_transaction
                .storage_as_mut::<M>()
                .replace(&given_key, state_value.as_ref())
                .unwrap();

            assert!(result.is_some());
        }

        /// Tests that putting a value creates Merkle metadata when empty
        pub fn test_put_creates_merkle_metadata_when_empty() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);

            // Given
            let key = M::generate_key(&M::primary_key(), rng);
            let state = M::generate_value(rng);

            // Write a contract state
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, state.as_ref())
                .unwrap();

            // Read the Merkle metadata
            let metadata = storage_transaction
                .storage_as_mut::<M::Metadata>()
                .get(&M::primary_key())
                .unwrap();

            assert!(metadata.is_some());
        }

        /// Tests that removing the last value deletes the Merkle metadata
        pub fn test_remove_deletes_merkle_metadata_when_empty() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);

            // Given
            let key = M::generate_key(&M::primary_key(), rng);
            let state = M::generate_value(rng);

            // Write a contract state
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, state.as_ref())
                .unwrap();

            // Read the Merkle metadata
            storage_transaction
                .storage_as_mut::<M::Metadata>()
                .get(&M::primary_key())
                .unwrap()
                .expect("Expected Merkle metadata to be present");

            // Remove the contract asset
            storage_transaction
                .storage_as_mut::<M>()
                .remove(&key)
                .unwrap();

            // Read the Merkle metadata
            let metadata = storage_transaction
                .storage_as_mut::<M::Metadata>()
                .get(&M::primary_key())
                .unwrap();

            assert!(metadata.is_none());
        }

        /// Tests that we can generate and validate merkle proofs
        pub fn test_can_generate_and_validate_proofs() {
            let mut storage = InMemoryStorage::<M::Column>::default();
            let mut storage_transaction = storage.write_transaction();

            let rng = &mut StdRng::seed_from_u64(1234);
            let current_key = M::primary_key();
            let key = M::generate_key(&current_key, rng);
            let state = M::generate_value(rng);

            let key_encoder = M::KeyCodec::encode(&key);
            let key_bytes = key_encoder.as_bytes();
            let merkle_key = MerkleTreeKey::new(&*key_bytes);
            let value_bytes = M::ValueCodec::encode_as_value(state.as_ref());

            // Write the state
            storage_transaction
                .storage_as_mut::<M>()
                .insert(&key, state.as_ref())
                .unwrap();

            // Read the first root
            let root = storage_transaction
                .storage_as_mut::<M>()
                .root(&current_key)
                .unwrap();

            let tree: MerkleTree<M::Nodes, _> =
                MerkleTree::load(&storage_transaction, &root)
                    .expect("could not load merkle tree");

            let Proof::Inclusion(inclusion_proof) = tree
                .generate_proof(&merkle_key)
                .expect("failed to generate proof")
            else {
                panic!("expected inclusion proof");
            };

            let proof_is_valid = inclusion_proof.verify(&root, &merkle_key, &value_bytes);

            assert!(proof_is_valid);
        }
    }

    /// Helper trait enabling referencing generics in
    /// SMT `TableWithBlueprint` implementations
    /// as associated types.
    pub trait SmtTableWithBlueprint:
        TableWithBlueprint<
        Blueprint = Sparse<
            Self::KeyCodec,
            Self::ValueCodec,
            Self::Metadata,
            Self::Nodes,
            Self::KeyConverter,
        >,
    >
    {
        /// The key codec type for encoding/decoding keys
        type KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>;

        /// The value codec type for encoding/decoding values
        type ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>;

        /// The metadata table type for storing SMT metadata
        type Metadata: TableWithBlueprint<
            Column = Self::Column,
            Value = SparseMerkleMetadata,
            OwnedValue = SparseMerkleMetadata,
        >;

        /// The nodes table type for storing merkle nodes
        type Nodes: TableWithBlueprint<
            Key = MerkleRoot,
            OwnedKey = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
            Column = Self::Column,
        >;

        /// The converter type for mapping column keys to SMT instances
        type KeyConverter: PrimaryKey<
            InputKey = Self::Key,
            OutputKey = <Self::Metadata as Mappable>::Key,
        >;
    }

    impl<T, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> SmtTableWithBlueprint for T
    where
        T: TableWithBlueprint<
            Blueprint = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>,
        >,
        KeyCodec: Encode<Self::Key> + Decode<Self::OwnedKey>,
        ValueCodec: Encode<Self::Value> + Decode<Self::OwnedValue>,
        Metadata: TableWithBlueprint<
            Column = Self::Column,
            Value = SparseMerkleMetadata,
            OwnedValue = SparseMerkleMetadata,
        >,
        Nodes: TableWithBlueprint<
            Key = MerkleRoot,
            OwnedKey = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
            Column = Self::Column,
        >,
        KeyConverter:
            PrimaryKey<InputKey = Self::Key, OutputKey = <Metadata as Mappable>::Key>,
    {
        type KeyCodec = KeyCodec;
        type ValueCodec = ValueCodec;
        type Metadata = Metadata;
        type Nodes = Nodes;
        type KeyConverter = KeyConverter;
    }

    /// Generates test functions for tables using the sparse merkle tree structure.
    #[cfg(feature = "test-helpers")]
    #[macro_export]
    macro_rules! root_storage_tests {
        ($table:ident) => {
            #[test]
            fn smt_storage__test_root() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_root();
            }

            #[test]
            fn smt_storage__test_root_returns_empty_root_for_empty_metadata() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_root_returns_empty_root_for_empty_metadata();
            }

            #[test]
            fn smt_storage__put_updates_the_state_merkle_root_for_the_given_metadata() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_put_updates_the_state_merkle_root_for_the_given_metadata();
            }

            #[test]
            fn smt_storage__test_remove_updates_the_state_merkle_root_for_the_given_metadata() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_remove_updates_the_state_merkle_root_for_the_given_metadata();
            }

            #[test]
            fn smt_storage__test_updating_foreign_metadata_does_not_affect_the_given_metadata_insertion() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_updating_foreign_metadata_does_not_affect_the_given_metadata_insertion();
            }

            #[test]
            fn smt_storage__test_put_creates_merkle_metadata_when_empty() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_put_creates_merkle_metadata_when_empty();
            }

            #[test]
            fn smt_storage__test_remove_deletes_merkle_metadata_when_empty() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_remove_deletes_merkle_metadata_when_empty();
            }

            #[test]
            fn smt_storage__test_can_generate_and_validate_proofs() {
                $crate::blueprint::sparse::root_storage_tests_smt::SmtTests::<$table>::test_can_generate_and_validate_proofs();
            }
        };
    }
}
