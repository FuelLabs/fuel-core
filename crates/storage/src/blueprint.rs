//! The module defines structures for the [`Mappable`] tables.
//! Each table may have its blueprint that defines how it works with the storage.
//! The table may have a plain blueprint that simply works in CRUD mode, or it may be an SMT-based
//! blueprint that maintains a valid Merkle tree over the storage entries.

use crate::{
    Mappable,
    Result as StorageResult,
};
use fuel_vm_private::prelude::MerkleRoot;
use std::borrow::Cow;

pub mod avl_merkle;
pub mod btree_merkle;
pub mod merklized;
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
pub trait BlueprintInspect<M, S>
where
    M: Mappable,
{
    /// Checks if the value exists in the storage.
    fn exists(storage: &S, key: &M::Key) -> StorageResult<bool>;

    /// Returns the value from the storage.
    fn get<'a>(
        storage: &'a S,
        key: &M::Key,
    ) -> StorageResult<Option<Cow<'a, M::OwnedValue>>>;
}

/// It is an extension of the [`BlueprintInspect`] that allows mutating the storage.
pub trait BlueprintMutate<M, S>: BlueprintInspect<M, S>
where
    M: Mappable,
{
    /// Puts the key-value pair into the storage.
    fn put(storage: &mut S, key: &M::Key, value: &M::Value) -> StorageResult<()>;

    /// Puts the key-value pair into the storage and returns the old value.
    fn replace(
        storage: &mut S,
        key: &M::Key,
        value: &M::Value,
    ) -> StorageResult<Option<M::OwnedValue>>;

    /// Takes the value from the storage and returns it.
    /// The value is removed from the storage.
    fn take(storage: &mut S, key: &M::Key) -> StorageResult<Option<M::OwnedValue>>;

    /// Removes the value from the storage.
    fn delete(storage: &mut S, key: &M::Key) -> StorageResult<()>;
}

/// It is an extension of the blueprint that allows supporting batch operations.
/// Usually, they are more performant than initializing/inserting/removing values one by one.
pub trait SupportsBatching<M, S>: BlueprintMutate<M, S>
where
    M: Mappable,
{
    /// Initializes the storage with a bunch of key-value pairs.
    /// In some cases, this method may be more performant than [`Self::insert`].
    fn init<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a;

    /// Inserts the batch of key-value pairs into the storage.
    fn insert<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a;

    /// Removes the batch of key-value pairs from the storage.
    fn remove<'a, Iter>(storage: &mut S, set: Iter) -> StorageResult<()>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a;
}

/// It is an extension of the blueprint that supporting creation of the Merkle tree over the storage.
pub trait SupportsMerkle<Key, M, S>: BlueprintInspect<M, S>
where
    Key: ?Sized,
    M: Mappable,
{
    /// Returns the root of the Merkle tree.
    fn root(storage: &S, key: &Key) -> StorageResult<MerkleRoot>;
}
