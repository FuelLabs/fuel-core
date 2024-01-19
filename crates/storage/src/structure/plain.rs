//! This module implements the plain structure for the storage.
//! The plain structure is the simplest one. It doesn't maintain any additional data structures
//! and doesn't provide any additional functionality. It is just a key-value store that encodes/decodes
//! the key and value and puts/takes them into/from the storage.

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
        WriteOperation,
    },
    structure::{
        Structure,
        SupportsBatching,
    },
    structured_storage::TableWithStructure,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
};

/// The type that represents the plain structure.
/// The `KeyCodec` and `ValueCodec` are used to encode/decode the key and value.
pub struct Plain<KeyCodec, ValueCodec> {
    _marker: core::marker::PhantomData<(KeyCodec, ValueCodec)>,
}

impl<M, S, KeyCodec, ValueCodec> Structure<M, S> for Plain<KeyCodec, ValueCodec>
where
    M: Mappable,
    S: KeyValueStore,
    KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>,
    ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>,
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

impl<M, S, KeyCodec, ValueCodec> SupportsBatching<M, S> for Plain<KeyCodec, ValueCodec>
where
    S: BatchOperations<Column = Column>,
    M: Mappable + TableWithStructure<Structure = Plain<KeyCodec, ValueCodec>>,
    M::Structure: Structure<M, S>,
{
    fn init(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()> {
        Self::insert(storage, column, set)
    }

    fn insert(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()> {
        storage.batch_write(&mut set.map(|(key, value)| {
            let key_encoder = <M::Structure as Structure<M, S>>::KeyCodec::encode(key);
            let key_bytes = key_encoder.as_bytes().to_vec();
            let value =
                <M::Structure as Structure<M, S>>::ValueCodec::encode_as_value(value);
            (key_bytes, column, WriteOperation::Insert(value))
        }))
    }

    fn remove(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = &M::Key>,
    ) -> StorageResult<()> {
        storage.batch_write(&mut set.map(|key| {
            let key_encoder = <M::Structure as Structure<M, S>>::KeyCodec::encode(key);
            let key_bytes = key_encoder.as_bytes().to_vec();
            (key_bytes, column, WriteOperation::Remove)
        }))
    }
}
