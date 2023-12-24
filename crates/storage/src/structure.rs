use crate::{
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    kv_store::{
        BatchOperations,
        KeyValueStore,
    },
    Mappable,
    Result as StorageResult,
};

pub mod plain;
pub mod sparse;

pub trait Structure<M, S>
where
    M: Mappable,
    S: KeyValueStore,
{
    type KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>;
    type ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>;

    fn put(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<()>;

    fn replace(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>>;

    fn take(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>>;

    fn delete(storage: &mut S, key: &M::Key, column: S::Column) -> StorageResult<()>;

    fn exists(storage: &S, key: &M::Key, column: S::Column) -> StorageResult<bool> {
        let key_encoder = Self::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.exists(key_bytes.as_ref(), column)
    }

    fn size_of_value(
        storage: &S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<usize>> {
        let key_encoder = Self::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.size_of_value(key_bytes.as_ref(), column)
    }

    fn get(
        storage: &S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>> {
        let key_encoder = Self::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage
            .get(key_bytes.as_ref(), column)?
            .map(|value| {
                Self::ValueCodec::decode_from_value(value).map_err(crate::Error::Codec)
            })
            .transpose()
    }
}

pub trait BatchStructure<M, S>: Structure<M, S>
where
    M: Mappable,
    S: BatchOperations,
{
    fn init(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()>;

    fn insert(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> StorageResult<()>;

    fn remove(
        storage: &mut S,
        column: S::Column,
        set: &mut dyn Iterator<Item = &M::Key>,
    ) -> StorageResult<()>;
}
