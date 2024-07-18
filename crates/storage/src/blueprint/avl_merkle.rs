//! The module defines the `AVLMerkle` blueprint for the storage.
//! The `AVLMerkle` blueprint implements the AVL merkle tree on top of the storage.
//! It is like a [`Plain`](super::plain::Plain) blueprint that builds the AVL
//! merkle tree parallel to the normal storage and maintains it.

use crate::{
    blueprint::{
        BlueprintInspect,
        BlueprintMutate,
        SupportsMerkle,
    },
    codec::{
        Encode,
        Encoder,
    },
    storage_interlayer::StorageInterlayer,
    tables::merkle::AVLMerkleMetadata,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::{
    avl::{
        AVLMerkleTree,
        AVLStorage,
        MerkleTreeError,
        StorageNode,
    },
    common::Bytes32,
};
use std::borrow::Cow;

/// The trait that allows to convert the key of the table into a prefix and unique key.
/// All entries of the AVL Merkle tree are prefixed with the same key, and for
/// optimization reasons, the prefix is excluded during the work of the tree.
pub trait PrefixedKey {
    /// Converts the key of the table into the prefix key and unique key.
    fn prefix(&self) -> (Bytes32, Bytes32);
}

/// The trait that allows to convert prefix and unique key into table key.
pub trait FromPrefix {
    /// Converts the key of the table into the prefix key and unique key.
    fn from_prefix(prefix: Bytes32, unique: Bytes32) -> Self;
}

/// The `AVLMerkle` blueprint builds the storage as a [`Plain`](super::plain::Plain)
/// blueprint and maintains the AVL merkle tree by the `Metadata` and `Nodes` tables.
///
/// It uses the `KeyCodec` and `ValueCodec` to encode/decode the key and value in the
/// same way as a plain blueprint.
///
/// The `Metadata` table stores the metadata of the tree(like a root of the tree),
/// and the `Nodes` table stores the tree's nodes. The AVL Merkle is built over the encoded
/// keys and values using the same encoding as for main key-value pairs.
pub struct AVLMerkle<Metadata, Nodes, Encoder> {
    _marker: core::marker::PhantomData<(Metadata, Nodes, Encoder)>,
}

impl<Metadata, Nodes, Encoder> AVLMerkle<Metadata, Nodes, Encoder>
where
    Metadata: Mappable<Value = AVLMerkleMetadata, OwnedValue = AVLMerkleMetadata>,
    Metadata::Key: From<Bytes32>,
    Nodes: Mappable<Value = StorageNode, OwnedValue = StorageNode>,
    Nodes::Key: PrefixedKey,
{
    fn insert_into_tree<S, M>(
        storage: &mut S,
        key: &Nodes::Key,
        value: &M::Value,
    ) -> StorageResult<()>
    where
        M: Mappable,
        Encoder: Encode<M::Value>,
        S: StorageMutate<Metadata, Error = StorageError>
            + AVLStorage<Nodes, StorageError = StorageError>,
    {
        let (prefix, unique) = key.prefix();
        let metadata_key = prefix.into();
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Cow<AVLMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(&metadata_key)?
            .unwrap_or_default();

        let root_node = *prev_metadata.root_node();
        let mut tree: AVLMerkleTree<_, Nodes> =
            AVLMerkleTree::load(prefix, root_node, storage);

        let value = Encoder::encode(value);
        tree.insert(unique, value.as_bytes())
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Generate new metadata for the updated tree
        let root_node = *tree.root_node();
        let storage = tree.into_storage();
        let metadata = AVLMerkleMetadata::new(root_node);
        storage
            .storage::<Metadata>()
            .insert(&metadata_key, &metadata)?;
        Ok(())
    }

    fn remove_from_tree<S>(storage: &mut S, key: &Nodes::Key) -> StorageResult<()>
    where
        S: StorageMutate<Metadata, Error = StorageError>
            + AVLStorage<Nodes, StorageError = StorageError>,
    {
        let (prefix, unique) = key.prefix();
        let metadata_key = prefix.into();
        // Get latest metadata entry for this `primary_key`
        let prev_metadata: Option<Cow<AVLMerkleMetadata>> =
            storage.storage::<Metadata>().get(&metadata_key)?;

        if let Some(prev_metadata) = prev_metadata {
            let root_node = *prev_metadata.root_node();

            let mut tree: AVLMerkleTree<_, Nodes> =
                AVLMerkleTree::load(prefix, root_node, storage);

            tree.delete(unique)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root_node = *tree.root_node();
            let storage = tree.into_storage();
            // Generate new metadata for the updated tree
            let metadata = AVLMerkleMetadata::new(root_node);
            storage
                .storage::<Metadata>()
                .insert(&metadata_key, &metadata)?;
        }

        Ok(())
    }
}

impl<M, S, Metadata, Nodes, Encoder> BlueprintInspect<M, S>
    for AVLMerkle<Metadata, Nodes, Encoder>
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
    for AVLMerkle<Metadata, Nodes, Encoder>
where
    M: Mappable,
    M::OwnedValue: Eq,
    Metadata: Mappable<Value = AVLMerkleMetadata, OwnedValue = AVLMerkleMetadata>,
    Metadata::Key: From<Bytes32>,
    Nodes: Mappable<Key = M::Key, Value = StorageNode, OwnedValue = StorageNode>,
    Nodes::Key: PrefixedKey,
    Encoder: Encode<M::Value>,
    S: StorageMutate<M, Error = StorageError>
        + StorageMutate<Metadata, Error = StorageError>
        + AVLStorage<Nodes, StorageError = StorageError>,
{
    fn put(storage: &mut S, key: &M::Key, value: &M::Value) -> StorageResult<()> {
        let old_value = storage.storage_as_mut::<M>().replace(key, value)?;

        if let Some(old_value) = old_value {
            let value = value.to_owned().into();
            if old_value == value {
                return Ok(());
            }
        }

        Self::insert_into_tree::<S, M>(storage, key, value)?;
        Ok(())
    }

    fn replace(
        storage: &mut S,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let old_value = storage.storage_as_mut::<M>().replace(key, value)?;

        let prev = if let Some(old_value) = old_value {
            let value = value.to_owned().into();
            if old_value == value {
                return Ok(Some(value));
            }
            Some(old_value)
        } else {
            None
        };
        Self::insert_into_tree::<S, M>(storage, key, value)?;

        Ok(prev)
    }

    fn take(storage: &mut S, key: &M::Key) -> StorageResult<Option<M::OwnedValue>> {
        let old_value = storage.storage_as_mut::<M>().take(key)?;

        if let Some(old_value) = old_value {
            Self::remove_from_tree(storage, key)?;
            Ok(Some(old_value))
        } else {
            Ok(None)
        }
    }

    fn delete(storage: &mut S, key: &M::Key) -> StorageResult<()> {
        let old = storage.storage_as_mut::<M>().take(key)?;
        if old.is_some() {
            Self::remove_from_tree(storage, key)
        } else {
            Ok(())
        }
    }
}

impl<M, S, Metadata, Nodes, Encoder> SupportsMerkle<Metadata::Key, M, S>
    for AVLMerkle<Metadata, Nodes, Encoder>
where
    M: Mappable,
    Metadata: Mappable<Value = AVLMerkleMetadata, OwnedValue = AVLMerkleMetadata>,
    S: StorageInspect<Metadata, Error = StorageError>,
    Self: BlueprintInspect<M, S>,
{
    fn root(storage: &S, key: &Metadata::Key) -> StorageResult<MerkleRoot> {
        let metadata: Option<Cow<AVLMerkleMetadata>> =
            storage.storage_as_ref::<Metadata>().get(key)?;
        let root = metadata
            .and_then(|metadata| *metadata.root_node())
            .map(|root| root.hash())
            .unwrap_or_else(|| [0; 32]);
        Ok(root)
    }
}

impl<S, M, StorageError> AVLStorage<M> for StorageInterlayer<S>
where
    Self: StorageMutate<M, Error = StorageError>,
    M: Mappable<Value = StorageNode, OwnedValue = StorageNode>,
    M::Key: Sized + FromPrefix,
{
    type StorageError = StorageError;

    fn get(
        &self,
        prefix: &Bytes32,
        key: &Bytes32,
    ) -> Result<Option<StorageNode>, MerkleTreeError<Self::StorageError>> {
        let storage_key = M::Key::from_prefix(*prefix, *key);
        let value = self
            .storage_as_ref::<M>()
            .get(&storage_key)
            .map_err(MerkleTreeError::StorageError)?
            .map(|value| value.into_owned());
        Ok(value)
    }

    fn set(
        &mut self,
        prefix: &Bytes32,
        key: &Bytes32,
        value: &StorageNode,
    ) -> Result<(), MerkleTreeError<Self::StorageError>> {
        let storage_key = M::Key::from_prefix(*prefix, *key);
        self.storage_as_mut::<M>()
            .insert(&storage_key, value)
            .map_err(MerkleTreeError::StorageError)
    }
}
