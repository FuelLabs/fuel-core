//! The module contains the [`StorageWithBlueprint`] wrapper around the key-value storage
//! that implements the storage traits for the tables with blueprint.

use crate::{
    blueprint::{
        BlueprintInspect,
        BlueprintMutate,
        SupportsBatching,
        SupportsMerkle,
    },
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        Value,
    },
    storage_interlayer::StorageInterlayer,
    transactional::{
        Changes,
        Modifiable,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    Result as StorageResult,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
    StorageWrite,
};
use std::borrow::Cow;

pub mod balances;
pub mod blocks;
pub mod coins;
pub mod contracts;

pub mod merkle_data;
pub mod messages;
pub mod sealed_block;
pub mod state;
pub mod transactions;
pub mod upgrades;

pub type StructuredStorage<S> = StorageWithBlueprint<StorageInterlayer<S>>;

/// The table can implement this trait to indicate that it has a blueprint.
/// It inherits the default implementation of the storage traits through the [`StorageWithBlueprint`]
/// for the table.
pub trait TableWithBlueprint: Mappable + Sized {
    /// The type of the blueprint used by the table.
    type Blueprint;
}

/// The wrapper around the key-value storage that implements the storage traits for the tables
/// with blueprint.
#[derive(Default, Debug, Clone)]
pub struct StorageWithBlueprint<S> {
    pub(crate) inner: S,
}

impl<S> StorageWithBlueprint<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self { inner: storage }
    }

    /// Returns the inner storage.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> AsRef<S> for StorageWithBlueprint<S> {
    fn as_ref(&self) -> &S {
        &self.inner
    }
}

impl<S> AsMut<S> for StorageWithBlueprint<S> {
    fn as_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S, M> StorageInspect<M> for StorageWithBlueprint<S>
where
    S: StorageInspect<M, Error = StorageError>,
    M: TableWithBlueprint,
    M::Blueprint: BlueprintInspect<M, S>,
{
    type Error = S::Error;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::get(&self.inner, key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::exists(&self.inner, key)
    }
}

impl<S, M> StorageMutate<M> for StorageWithBlueprint<S>
where
    S: StorageMutate<M, Error = StorageError>,
    M: TableWithBlueprint,
    M::Blueprint: BlueprintMutate<M, S>,
{
    fn insert(&mut self, key: &M::Key, value: &M::Value) -> Result<(), Self::Error> {
        <M as TableWithBlueprint>::Blueprint::put(&mut self.inner, key, value)
    }

    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::replace(&mut self.inner, key, value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<(), Self::Error> {
        <M as TableWithBlueprint>::Blueprint::delete(&mut self.inner, key)
    }

    fn take(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::take(&mut self.inner, key)
    }
}

impl<S, M> StorageSize<M> for StorageWithBlueprint<S>
where
    S: StorageSize<M, Error = StorageError>,
    M: TableWithBlueprint<Value = [u8]>,
    M::Blueprint: BlueprintInspect<M, S>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        self.inner.size_of_value(key)
    }
}

impl<S, M> StorageBatchMutate<M> for StorageWithBlueprint<S>
where
    S: StorageMutate<M, Error = StorageError>,
    M: TableWithBlueprint,
    M::Blueprint: SupportsBatching<M, S>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::init(&mut self.inner, set)
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::insert(&mut self.inner, set)
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::remove(&mut self.inner, set)
    }
}

impl<Key, S, M> MerkleRootStorage<Key, M> for StorageWithBlueprint<S>
where
    S: StorageInspect<M, Error = StorageError>,
    M: TableWithBlueprint,
    M::Blueprint: SupportsMerkle<Key, M, S>,
{
    fn root(&self, key: &Key) -> Result<MerkleRoot, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::root(&self.inner, key)
    }
}

impl<S, M> StorageRead<M> for StorageWithBlueprint<S>
where
    S: StorageRead<M, Error = StorageError>,
    M: TableWithBlueprint<Value = [u8]>,
    M::Blueprint: BlueprintInspect<M, S>,
{
    fn read(
        &self,
        key: &<M as Mappable>::Key,
        buf: &mut [u8],
    ) -> Result<Option<usize>, Self::Error> {
        self.inner.read(key, buf)
    }

    fn read_alloc(
        &self,
        key: &<M as Mappable>::Key,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inner.read_alloc(key)
    }
}

impl<S, M> StorageWrite<M> for StorageWithBlueprint<S>
where
    S: StorageWrite<M, Error = StorageError>,
    M: TableWithBlueprint<Value = [u8]>,
    M::Blueprint: BlueprintMutate<M, S>,
    // TODO: Add new methods to the `Blueprint` that allows work with bytes directly
    //  without deserialization into `OwnedValue`.
    M::OwnedValue: Into<Vec<u8>>,
{
    fn write_bytes(&mut self, key: &M::Key, buf: &[u8]) -> Result<usize, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::put(&mut self.inner, key, buf)
            .map(|_| buf.len())
    }

    fn replace_bytes(
        &mut self,
        key: &M::Key,
        buf: &[u8],
    ) -> Result<(usize, Option<Vec<u8>>), Self::Error> {
        let bytes_written = buf.len();
        let prev =
            <M as TableWithBlueprint>::Blueprint::replace(&mut self.inner, key, buf)?
                .map(|prev| prev.into());
        let result = (bytes_written, prev);
        Ok(result)
    }

    fn take_bytes(&mut self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let take = <M as TableWithBlueprint>::Blueprint::take(&mut self.inner, key)?
            .map(|value| value.into());
        Ok(take)
    }
}

impl<S> KeyValueInspect for StorageWithBlueprint<S>
where
    S: KeyValueInspect,
{
    type Column = S::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.inner.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.inner.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.inner.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.inner.read(key, column, buf)
    }
}

impl<S> IterableStore for StorageWithBlueprint<S>
where
    S: IterableStore,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.inner.iter_store(column, prefix, start, direction)
    }
}

impl<S> Modifiable for StorageWithBlueprint<S>
where
    S: Modifiable,
{
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.inner.commit_changes(changes)
    }
}

/// The module that provides helper macros for testing the structured storage.
#[cfg(feature = "test-helpers")]
pub mod test {
    use crate as fuel_core_storage;
    use crate::kv_store::{
        KeyValueInspect,
        StorageColumn,
    };
    use fuel_core_storage::{
        kv_store::Value,
        Result as StorageResult,
    };
    use std::collections::HashMap;

    type Storage = HashMap<(u32, Vec<u8>), Value>;

    /// The in-memory storage for testing purposes.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InMemoryStorage<Column> {
        pub(crate) storage: Storage,
        _marker: core::marker::PhantomData<Column>,
    }

    impl<Column> InMemoryStorage<Column> {
        /// Returns the inner storage.
        pub fn storage(&self) -> &Storage {
            &self.storage
        }
    }

    impl<Column> Default for InMemoryStorage<Column> {
        fn default() -> Self {
            Self {
                storage: Default::default(),
                _marker: Default::default(),
            }
        }
    }

    impl<Column> KeyValueInspect for InMemoryStorage<Column>
    where
        Column: StorageColumn,
    {
        type Column = Column;

        fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
            let value = self.storage.get(&(column.id(), key.to_vec())).cloned();
            Ok(value)
        }
    }
}
