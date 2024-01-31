//! The module defines the `Sparse` blueprint for the storage.
//! The `Sparse` blueprint implements the sparse merkle tree on top of the storage.
//! It is like a [`Plain`](super::plain::Plain) blueprint that builds the sparse
//! merkle tree parallel to the normal storage and maintains it.

use crate::{
    blueprint::{
        Blueprint,
        SupportsBatching,
    },
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    column::Column,
    kv_store::{
        BatchOperations,
        KeyValueStore,
        StorageColumn,
        WriteOperation,
    },
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    tables::merkle::SparseMerkleMetadata,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
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
use std::borrow::Cow;

/// The trait that allows to convert the key of the table into the key of the metadata table.
/// If the key comprises several entities, it is possible to build a Merkle tree over different primary keys.
/// The trait defines the key over which to build an SMT.
pub trait PrimaryKey {
    /// The storage key of the table.
    type InputKey: ?Sized;
    /// The extracted primary key.
    type OutputKey: ?Sized;

    /// Converts the key of the table into the primary key of the metadata table.
    fn primary_key(key: &Self::InputKey) -> &Self::OutputKey;
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
        for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        KeyConverter: PrimaryKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let mut storage = StructuredStorage::new(storage);
        let primary_key = KeyConverter::primary_key(key);
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key)?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        tree.update(MerkleTreeKey::new(key_bytes), value_bytes)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Generate new metadata for the updated tree
        let root = tree.root();
        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key, &metadata)?;
        Ok(())
    }

    fn remove_from_tree<S, K>(
        storage: &mut S,
        key: &K,
        key_bytes: &[u8],
    ) -> StorageResult<()>
    where
        K: ?Sized,
        for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
            + StorageMutate<Nodes, Error = StorageError>,
        KeyConverter: PrimaryKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let mut storage = StructuredStorage::new(storage);
        let primary_key = KeyConverter::primary_key(key);
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Option<Cow<SparseMerkleMetadata>> =
            storage.storage::<Metadata>().get(primary_key)?;

        if let Some(prev_metadata) = prev_metadata {
            let root = *prev_metadata.root();

            let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root = tree.root();
            if &root == MerkleTree::<Nodes, S>::empty_root() {
                // The tree is now empty; remove the metadata
                storage.storage::<Metadata>().remove(primary_key)?;
            } else {
                // Generate new metadata for the updated tree
                let metadata = SparseMerkleMetadata::new(root);
                storage
                    .storage::<Metadata>()
                    .insert(primary_key, &metadata)?;
            }
        }

        Ok(())
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> Blueprint<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    M: Mappable,
    S: KeyValueStore,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
        Key = MerkleRoot,
        Value = sparse::Primitive,
        OwnedValue = sparse::Primitive,
    >,
    KeyConverter: PrimaryKey<InputKey = M::Key, OutputKey = Metadata::Key>,
    for<'a> StructuredStorage<&'a mut S>: StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    type KeyCodec = KeyCodec;
    type ValueCodec = ValueCodec;

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
        let prev = storage
            .replace(key_bytes.as_ref(), column, value.clone())?
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
        let prev = storage
            .take(key_bytes.as_ref(), column)?
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
    MerkleRootStorage<Metadata::Key, M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable
        + TableWithBlueprint<
            Blueprint = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>,
        >,
    Self: StorageMutate<M, Error = StorageError>
        + StorageInspect<Metadata, Error = StorageError>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Metadata::Key: Sized,
{
    fn root(&self, key: &Metadata::Key) -> StorageResult<MerkleRoot> {
        use crate::StorageAsRef;
        let metadata: Option<Cow<SparseMerkleMetadata>> =
            self.storage_as_ref::<Metadata>().get(key)?;
        let root = metadata
            .map(|metadata| *metadata.root())
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        Ok(root)
    }
}

type NodeKeyCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as Blueprint<Nodes, S>>::KeyCodec;
type NodeValueCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as Blueprint<Nodes, S>>::ValueCodec;

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter> SupportsBatching<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>
where
    S: BatchOperations<Column = Column>,
    M: Mappable
        + TableWithBlueprint<
            Blueprint = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConverter>,
        >,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
            Key = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
        > + TableWithBlueprint,
    KeyConverter: PrimaryKey<InputKey = M::Key, OutputKey = Metadata::Key>,
    Nodes::Blueprint: Blueprint<Nodes, S>,
    for<'a> StructuredStorage<&'a mut S>: StorageMutate<M, Error = StorageError>
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

        let mut storage = StructuredStorage::new(storage);

        if storage.storage::<Metadata>().contains_key(primary_key)? {
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

        storage.as_mut().batch_write(
            &mut encoded_set
                .into_iter()
                .map(|(key, value)| (key, column, WriteOperation::Insert(value.into()))),
        )?;

        let mut nodes = nodes.iter().map(|(key, value)| {
            let key = NodeKeyCodec::<S, Nodes>::encode(key)
                .as_bytes()
                .into_owned();
            let value = NodeValueCodec::<S, Nodes>::encode_as_value(value);
            (key, Nodes::column(), WriteOperation::Insert(value))
        });
        storage.as_mut().batch_write(&mut nodes)?;

        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key, &metadata)?;

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

        let mut storage = StructuredStorage::new(storage);
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key)?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
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

        storage.as_mut().batch_write(
            &mut encoded_set
                .into_iter()
                .map(|(key, value)| (key, column, WriteOperation::Insert(value.into()))),
        )?;

        // Generate new metadata for the updated tree
        let metadata = SparseMerkleMetadata::new(root);
        storage
            .storage::<Metadata>()
            .insert(primary_key, &metadata)?;

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

        let mut storage = StructuredStorage::new(storage);
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(primary_key)?
            .unwrap_or_default();

        let root = *prev_metadata.root();
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        let encoded_set = set
            .map(|key| KeyCodec::encode(key).as_bytes().into_owned())
            .collect_vec();

        for key_bytes in encoded_set.iter() {
            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        }
        let root = tree.root();

        storage.as_mut().batch_write(
            &mut encoded_set
                .into_iter()
                .map(|key| (key, column, WriteOperation::Remove)),
        )?;

        if &root == MerkleTree::<Nodes, S>::empty_root() {
            // The tree is now empty; remove the metadata
            storage.storage::<Metadata>().remove(primary_key)?;
        } else {
            // Generate new metadata for the updated tree
            let metadata = SparseMerkleMetadata::new(root);
            storage
                .storage::<Metadata>()
                .insert(primary_key, &metadata)?;
        }

        Ok(())
    }
}
