use crate::{
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
    structure::{
        BatchStructure,
        Structure,
    },
    structured_storage::{
        StructuredStorage,
        TableWithStructure,
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

pub trait MetadataKey {
    type InputKey: ?Sized;
    type OutputKey: ?Sized;

    fn metadata_key(key: &Self::InputKey) -> &Self::OutputKey;
}

pub struct Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor> {
    _marker:
        core::marker::PhantomData<(KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor)>,
}

impl<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>
    Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>
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
        KeyConvertor: MetadataKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let mut storage = StructuredStorage::new(storage);
        let metadata_key = KeyConvertor::metadata_key(key);
        // Get latest metadata entry for this `metadata_key`
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(metadata_key)?
            .unwrap_or_default();

        let root = prev_metadata.root;
        let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        tree.update(MerkleTreeKey::new(key_bytes), value_bytes)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Generate new metadata for the updated tree
        let root = tree.root();
        let metadata = SparseMerkleMetadata { root };
        storage
            .storage::<Metadata>()
            .insert(metadata_key, &metadata)?;
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
        KeyConvertor: MetadataKey<InputKey = K, OutputKey = Metadata::Key>,
    {
        let mut storage = StructuredStorage::new(storage);
        let metadata_key = KeyConvertor::metadata_key(key);
        // Get latest metadata entry for this `metadata_key`
        let prev_metadata: Option<Cow<SparseMerkleMetadata>> =
            storage.storage::<Metadata>().get(metadata_key)?;

        if let Some(prev_metadata) = prev_metadata {
            let root = prev_metadata.root;

            let mut tree: MerkleTree<Nodes, _> = MerkleTree::load(&mut storage, &root)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root = tree.root();
            if &root == MerkleTree::<Nodes, S>::empty_root() {
                // The tree is now empty; remove the metadata
                storage.storage::<Metadata>().remove(metadata_key)?;
            } else {
                // Generate new metadata for the updated tree
                let metadata = SparseMerkleMetadata { root };
                storage
                    .storage::<Metadata>()
                    .insert(metadata_key, &metadata)?;
            }
        }

        Ok(())
    }
}

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor> Structure<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>
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
    KeyConvertor: MetadataKey<InputKey = M::Key, OutputKey = Metadata::Key>,
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

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>
    MerkleRootStorage<Metadata::Key, M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable
        + TableWithStructure<
            Structure = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>,
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
            .map(|metadata| metadata.root)
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        Ok(root)
    }
}

type NodeKeyCodec<S, Nodes> =
    <<Nodes as TableWithStructure>::Structure as Structure<Nodes, S>>::KeyCodec;
type NodeValueCodec<S, Nodes> =
    <<Nodes as TableWithStructure>::Structure as Structure<Nodes, S>>::ValueCodec;

impl<M, S, KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor> BatchStructure<M, S>
    for Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>
where
    S: BatchOperations<Column = Column>,
    M: Mappable
        + TableWithStructure<
            Structure = Sparse<KeyCodec, ValueCodec, Metadata, Nodes, KeyConvertor>,
        >,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Metadata: Mappable<Value = SparseMerkleMetadata, OwnedValue = SparseMerkleMetadata>,
    Nodes: Mappable<
            Key = MerkleRoot,
            Value = sparse::Primitive,
            OwnedValue = sparse::Primitive,
        > + TableWithStructure,
    KeyConvertor: MetadataKey<InputKey = M::Key, OutputKey = Metadata::Key>,
    Nodes::Structure: Structure<Nodes, S>,
    for<'a> StructuredStorage<&'a mut S>: StorageMutate<M, Error = StorageError>
        + StorageMutate<Metadata, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>,
{
    fn init(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()> {
        let mut set = set.peekable();

        let metadata_key;
        if let Some((key, _)) = set.peek() {
            metadata_key = KeyConvertor::metadata_key(*key);
        } else {
            return Ok(())
        }

        let mut storage = StructuredStorage::new(storage);

        if storage.storage::<Metadata>().contains_key(metadata_key)? {
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

        let metadata = SparseMerkleMetadata { root };
        storage
            .storage::<Metadata>()
            .insert(metadata_key, &metadata)?;

        Ok(())
    }

    fn insert(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()> {
        let mut set = set.peekable();

        let metadata_key;
        if let Some((key, _)) = set.peek() {
            metadata_key = KeyConvertor::metadata_key(*key);
        } else {
            return Ok(())
        }

        let mut storage = StructuredStorage::new(storage);
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(metadata_key)?
            .unwrap_or_default();

        let root = prev_metadata.root;
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
        let metadata = SparseMerkleMetadata { root };
        storage
            .storage::<Metadata>()
            .insert(metadata_key, &metadata)?;

        Ok(())
    }

    fn remove(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = &M::Key>,
    ) -> StorageResult<()> {
        let mut set = set.peekable();

        let metadata_key;
        if let Some(key) = set.peek() {
            metadata_key = KeyConvertor::metadata_key(*key);
        } else {
            return Ok(())
        }

        let mut storage = StructuredStorage::new(storage);
        let prev_metadata: Cow<SparseMerkleMetadata> = storage
            .storage::<Metadata>()
            .get(metadata_key)?
            .unwrap_or_default();

        let root = prev_metadata.root;
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
            storage.storage::<Metadata>().remove(metadata_key)?;
        } else {
            // Generate new metadata for the updated tree
            let metadata = SparseMerkleMetadata { root };
            storage
                .storage::<Metadata>()
                .insert(metadata_key, &metadata)?;
        }

        Ok(())
    }
}
