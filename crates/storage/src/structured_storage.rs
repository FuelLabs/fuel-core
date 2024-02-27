//! The module contains the [`StructuredStorage`] wrapper around the key-value storage
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
        BatchOperations,
        KVItem,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
        Value,
        WriteOperation,
    },
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
    StorageSize,
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

/// The table can implement this trait to indicate that it has a blueprint.
/// It inherits the default implementation of the storage traits through the [`StructuredStorage`]
/// for the table.
pub trait TableWithBlueprint: Mappable + Sized {
    /// The type of the blueprint used by the table.
    type Blueprint;
    /// The column type used by the table.
    type Column: StorageColumn;

    /// The column occupied by the table.
    fn column() -> Self::Column;
}

/// The wrapper around the key-value storage that implements the storage traits for the tables
/// with blueprint.
#[derive(Default, Debug, Clone)]
pub struct StructuredStorage<S> {
    pub(crate) inner: S,
}

impl<S> StructuredStorage<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self { inner: storage }
    }
}

impl<S> AsRef<S> for StructuredStorage<S> {
    fn as_ref(&self) -> &S {
        &self.inner
    }
}

impl<S> AsMut<S> for StructuredStorage<S> {
    fn as_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S> KeyValueInspect for StructuredStorage<S>
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

impl<S> KeyValueMutate for StructuredStorage<S>
where
    S: KeyValueMutate,
{
    fn put(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<()> {
        self.inner.put(key, column, value)
    }

    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        self.inner.replace(key, column, value)
    }

    fn write(
        &mut self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        self.inner.write(key, column, buf)
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.inner.take(key, column)
    }

    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        self.inner.delete(key, column)
    }
}

impl<S> BatchOperations for StructuredStorage<S>
where
    S: BatchOperations,
{
    fn batch_write<I>(&mut self, column: Self::Column, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, WriteOperation)>,
    {
        self.inner.batch_write(column, entries)
    }
}

impl<S> IterableStore for StructuredStorage<S>
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

impl<S> Modifiable for StructuredStorage<S>
where
    S: Modifiable,
{
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.inner.commit_changes(changes)
    }
}

impl<Column, S, M> StorageInspect<M> for StructuredStorage<S>
where
    S: KeyValueInspect<Column = Column>,
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: BlueprintInspect<M, StructuredStorage<S>>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::get(self, key, M::column())
            .map(|value| value.map(Cow::Owned))
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::exists(self, key, M::column())
    }
}

impl<Column, S, M> StorageMutate<M> for StructuredStorage<S>
where
    S: KeyValueMutate<Column = Column>,
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: BlueprintMutate<M, StructuredStorage<S>>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::replace(self, key, M::column(), value)
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::take(self, key, M::column())
    }
}

impl<Column, S, M> StorageSize<M> for StructuredStorage<S>
where
    S: KeyValueInspect<Column = Column>,
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: BlueprintInspect<M, StructuredStorage<S>>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::size_of_value(self, key, M::column())
    }
}

impl<Column, S, M> StorageBatchMutate<M> for StructuredStorage<S>
where
    S: BatchOperations<Column = Column>,
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: SupportsBatching<M, StructuredStorage<S>>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::init(self, M::column(), set)
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::insert(self, M::column(), set)
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::remove(self, M::column(), set)
    }
}

impl<Column, Key, S, M> MerkleRootStorage<Key, M> for StructuredStorage<S>
where
    S: KeyValueInspect<Column = Column>,
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: SupportsMerkle<Key, M, StructuredStorage<S>>,
{
    fn root(&self, key: &Key) -> Result<MerkleRoot, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::root(self, key)
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
    use std::collections::BTreeMap;

    type Storage = BTreeMap<(u32, Vec<u8>), Value>;

    /// The in-memory storage for testing purposes.
    #[derive(Debug, PartialEq, Eq)]
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
