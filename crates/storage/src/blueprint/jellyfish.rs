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
    Error as StorageError,
    Mappable,
    MerkleRoot,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::{
    jellyfish,
    jellyfish::{
        in_memory,
        merkle_tree::JellyfishMerkleTreeStorage,
    },
    sparse::MerkleTreeKey,
};
use itertools::Itertools;

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

/// The `Jellyfish` blueprint that implements the Jellyfish Merkle Tree on top of the storage.
pub struct Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion> {
    _marker: core::marker::PhantomData<(
        KeyCodec,
        ValueCodec,
        Nodes,
        Preimages,
        LatestVersion,
    )>,
}

impl<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
    Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
where
    Nodes: Mappable<
        Key = jellyfish::jmt::storage::NodeKey,
        Value = jellyfish::jmt::storage::Node,
        OwnedValue = jellyfish::jmt::storage::Node,
    >,
    Preimages: Mappable<
        Key = jellyfish::jmt::KeyHash,
        Value = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
        OwnedValue = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
    >,
    LatestVersion: Mappable<Key = (), Value = u64, OwnedValue = u64>,
{
    fn insert_into_tree<S, K>(
        storage: &mut S,
        _key: &K,
        key_bytes: &[u8],
        value_bytes: &[u8],
    ) -> StorageResult<()>
    where
        K: ?Sized,
        S: StorageMutate<Nodes, Error = StorageError>
            + StorageMutate<Preimages, Error = StorageError>
            + StorageMutate<LatestVersion, Error = StorageError>,
    {
        let mut tree: JellyfishMerkleTreeStorage<
            Nodes,
            Preimages,
            LatestVersion,
            &mut S,
            StorageError,
        > = JellyfishMerkleTreeStorage::load_no_check(storage)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        tree.update(MerkleTreeKey::new(key_bytes), value_bytes)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        Ok(())
    }

    fn remove_from_tree<S, K>(
        storage: &mut S,
        _key: &K,
        key_bytes: &[u8],
    ) -> StorageResult<()>
    where
        K: ?Sized,
        S: StorageMutate<Nodes, Error = StorageError>
            + StorageMutate<Preimages, Error = StorageError>
            + StorageMutate<LatestVersion, Error = StorageError>,
    {
        let mut tree: JellyfishMerkleTreeStorage<
            Nodes,
            Preimages,
            LatestVersion,
            &mut S,
            StorageError,
        > = JellyfishMerkleTreeStorage::load_no_check(storage)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        tree.delete(MerkleTreeKey::new(key_bytes))
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        Ok(())
    }
}

impl<M, S, KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion> BlueprintInspect<M, S>
    for Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
where
    M: Mappable,
    S: KeyValueInspect,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
{
    type KeyCodec = KeyCodec;
    type ValueCodec = ValueCodec;
}

impl<M, S, KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion> BlueprintMutate<M, S>
    for Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
where
    M: Mappable,
    S: KeyValueMutate,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Nodes: Mappable<
        Key = jellyfish::jmt::storage::NodeKey,
        Value = jellyfish::jmt::storage::Node,
        OwnedValue = jellyfish::jmt::storage::Node,
    >,
    Preimages: Mappable<
        Key = jellyfish::jmt::KeyHash,
        Value = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
        OwnedValue = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
    >,
    LatestVersion: Mappable<Key = (), Value = u64, OwnedValue = u64>,
    S: StorageMutate<Nodes, Error = StorageError>
        + StorageMutate<Preimages, Error = StorageError>
        + StorageMutate<LatestVersion, Error = StorageError>,
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

impl<M, S, KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion> SupportsMerkle<(), M, S>
    for Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
where
    M: Mappable,
    S: KeyValueInspect,
    Nodes: Mappable<
        Key = jellyfish::jmt::storage::NodeKey,
        Value = jellyfish::jmt::storage::Node,
        OwnedValue = jellyfish::jmt::storage::Node,
    >,
    Preimages: Mappable<
        Key = jellyfish::jmt::KeyHash,
        Value = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
        OwnedValue = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
    >,
    LatestVersion: Mappable<Key = (), Value = u64, OwnedValue = u64>,
    S: StorageInspect<Nodes, Error = StorageError>
        + StorageInspect<Preimages, Error = StorageError>
        + StorageInspect<LatestVersion, Error = StorageError>,
    Self: BlueprintInspect<M, S>,
{
    fn root(storage: &S, _key: &()) -> StorageResult<MerkleRoot> {
        let tree: JellyfishMerkleTreeStorage<
            Nodes,
            Preimages,
            LatestVersion,
            &S,
            StorageError,
        > = JellyfishMerkleTreeStorage::load_no_check(storage)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        let root = tree
            .root()
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        Ok(root)
    }
}

type NodeKeyCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as BlueprintInspect<Nodes, S>>::KeyCodec;
type NodeValueCodec<S, Nodes> =
    <<Nodes as TableWithBlueprint>::Blueprint as BlueprintInspect<Nodes, S>>::ValueCodec;

impl<Column, M, S, KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
    SupportsBatching<M, S>
    for Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>
where
    Column: StorageColumn,
    S: BatchOperations<Column = Column>,
    M: TableWithBlueprint<
        Blueprint = Jellyfish<KeyCodec, ValueCodec, Nodes, Preimages, LatestVersion>,
        Column = Column,
    >,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
    Nodes: Mappable<
            Key = jellyfish::jmt::storage::NodeKey,
            Value = jellyfish::jmt::storage::Node,
            OwnedValue = jellyfish::jmt::storage::Node,
        > + TableWithBlueprint<Column = Column>,
    Preimages: Mappable<
        Key = jellyfish::jmt::KeyHash,
        Value = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
        OwnedValue = (jellyfish::jmt::Version, jellyfish::jmt::OwnedValue),
    >,
    LatestVersion: Mappable<Key = (), Value = u64, OwnedValue = u64>,
    Nodes::Blueprint: BlueprintInspect<Nodes, S>,
    S: StorageMutate<M, Error = StorageError>
        + StorageMutate<Nodes, Error = StorageError>
        + StorageMutate<Preimages, Error = StorageError>
        + StorageMutate<LatestVersion, Error = StorageError>,
{
    fn init<'a, Iter>(storage: &mut S, column: S::Column, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        let set = set.peekable();

        let encoded_set = set
            .map(|(key, value)| {
                let key = KeyCodec::encode(key).as_bytes().into_owned();
                let value = ValueCodec::encode(value).as_bytes().into_owned();
                (key, value)
            })
            .collect_vec();

        let (_root, nodes) = in_memory::MerkleTree::nodes_from_set(
            encoded_set
                .iter()
                .map(|(key, value)| (MerkleTreeKey::new(key), value)),
        )
        .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

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
        let set = set.peekable();

        let mut tree: JellyfishMerkleTreeStorage<
            Nodes,
            Preimages,
            LatestVersion,
            &mut S,
            StorageError,
        > = JellyfishMerkleTreeStorage::load_no_check(storage)
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
        let mut storage = tree.storage_write();

        storage.batch_write(
            column,
            encoded_set
                .into_iter()
                .map(|(key, value)| (key, WriteOperation::Insert(value.into()))),
        )?;

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
        let set = set.peekable();

        let mut tree: JellyfishMerkleTreeStorage<
            Nodes,
            Preimages,
            LatestVersion,
            &mut S,
            StorageError,
        > = JellyfishMerkleTreeStorage::load_no_check(storage)
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        let encoded_set = set
            .map(|key| KeyCodec::encode(key).as_bytes().into_owned())
            .collect_vec();

        for key_bytes in encoded_set.iter() {
            tree.delete(MerkleTreeKey::new(key_bytes))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        }
        let mut storage = tree.storage_write();

        storage.batch_write(
            column,
            encoded_set
                .into_iter()
                .map(|key| (key, WriteOperation::Remove)),
        )?;

        Ok(())
    }
}
