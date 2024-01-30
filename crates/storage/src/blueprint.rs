//! The module defines structures for the [`Mappable`] tables.
//! Each table may have its blueprint that defines how it works with the storage.
//! The table may have a plain blueprint that simply works in CRUD mode, or it may be an SMT-based
//! blueprint that maintains a valid Merkle tree over the storage entries.

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

/// This trait allows defining the agnostic implementation for all storage
/// traits(`StorageInspect,` `StorageMutate,` etc) while the main logic is
/// hidden inside the blueprint. It allows quickly adding support for new
/// structures only by implementing the trait and reusing the existing
/// infrastructure in other places. It allows changing the blueprint on the
/// fly in the definition of the table without affecting other areas of the codebase.
///
/// The blueprint is responsible for encoding/decoding(usually it is done via `KeyCodec` and `ValueCodec`)
/// the key and value and putting/extracting it to/from the storage.
pub trait Blueprint<M, S>
where
    M: Mappable,
    S: KeyValueStore,
{
    /// The codec used to encode and decode storage key.
    type KeyCodec: Encode<M::Key> + Decode<M::OwnedKey>;
    /// The codec used to encode and decode storage value.
    type ValueCodec: Encode<M::Value> + Decode<M::OwnedValue>;

    /// Puts the key-value pair into the storage.
    fn put(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<()>;

    /// Puts the key-value pair into the storage and returns the old value.
    fn replace(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// Takes the value from the storage and returns it.
    /// The value is removed from the storage.
    fn take(
        storage: &mut S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// Removes the value from the storage.
    fn delete(storage: &mut S, key: &M::Key, column: S::Column) -> StorageResult<()>;

    /// Checks if the value exists in the storage.
    fn exists(storage: &S, key: &M::Key, column: S::Column) -> StorageResult<bool> {
        let key_encoder = Self::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.exists(key_bytes.as_ref(), column)
    }

    /// Returns the size of the value in the storage.
    fn size_of_value(
        storage: &S,
        key: &M::Key,
        column: S::Column,
    ) -> StorageResult<Option<usize>> {
        let key_encoder = Self::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        storage.size_of_value(key_bytes.as_ref(), column)
    }

    /// Returns the value from the storage.
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

/// It is an extension of the blueprint that allows supporting batch operations.
/// Usually, they are more performant than initializing/inserting/removing values one by one.
pub trait SupportsBatching<M, S>: Blueprint<M, S>
where
    M: Mappable,
    S: BatchOperations,
{
    /// Initializes the storage with a bunch of key-value pairs.
    /// In some cases, this method may be more performant than [`Self::insert`].
    fn init<'a, Iter>(storage: &mut S, column: S::Column, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a;

    /// Inserts the batch of key-value pairs into the storage.
    fn insert<'a, Iter>(
        storage: &mut S,
        column: S::Column,
        set: Iter,
    ) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a;

    /// Removes the batch of key-value pairs from the storage.
    fn remove<'a, Iter>(
        storage: &mut S,
        column: S::Column,
        set: Iter,
    ) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a;
}
