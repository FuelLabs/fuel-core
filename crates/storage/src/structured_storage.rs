//! The module contains the [`StructuredStorage`] wrapper around the key-value storage
//! that implements the storage traits for the tables with blueprint.

use crate::{
    blueprint::{
        Blueprint,
        SupportsBatching,
        SupportsMerkle,
    },
    kv_store::{
        BatchOperations,
        KeyValueStore,
        StorageColumn,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
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
#[derive(Clone, Debug)]
pub struct StructuredStorage<S> {
    pub(crate) storage: S,
}

impl<S> StructuredStorage<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S> AsRef<S> for StructuredStorage<S> {
    fn as_ref(&self) -> &S {
        &self.storage
    }
}

impl<S> AsMut<S> for StructuredStorage<S> {
    fn as_mut(&mut self) -> &mut S {
        &mut self.storage
    }
}

impl<Column, S, M> StorageInspect<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint<Column = Column>,
    M::Blueprint: Blueprint<M, S>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::get(&self.storage, key, M::column())
            .map(|value| value.map(Cow::Owned))
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::exists(&self.storage, key, M::column())
    }
}

impl<Column, S, M> StorageMutate<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint<Column = Column>,
    M::Blueprint: Blueprint<M, S>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::replace(
            &mut self.storage,
            key,
            M::column(),
            value,
        )
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::take(&mut self.storage, key, M::column())
    }
}

impl<Column, S, M> StorageSize<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint<Column = Column>,
    M::Blueprint: Blueprint<M, S>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::size_of_value(
            &self.storage,
            key,
            M::column(),
        )
    }
}

impl<Column, S, M> StorageBatchMutate<M> for StructuredStorage<S>
where
    S: BatchOperations<Column = Column>,
    M: Mappable + TableWithBlueprint<Column = Column>,
    M::Blueprint: SupportsBatching<M, S>,
{
    fn init_storage<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::init(&mut self.storage, M::column(), set)
    }

    fn insert_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = (&'a M::Key, &'a M::Value)>,
        M::Key: 'a,
        M::Value: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::insert(&mut self.storage, M::column(), set)
    }

    fn remove_batch<'a, Iter>(&mut self, set: Iter) -> Result<(), Self::Error>
    where
        Iter: 'a + Iterator<Item = &'a M::Key>,
        M::Key: 'a,
    {
        <M as TableWithBlueprint>::Blueprint::remove(&mut self.storage, M::column(), set)
    }
}

impl<Column, Key, S, M> MerkleRootStorage<Key, M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint<Column = Column>,
    M::Blueprint: SupportsMerkle<Key, M, S>,
{
    fn root(&self, key: &Key) -> Result<MerkleRoot, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::root(&self.storage, key)
    }
}

/// The module that provides helper macros for testing the structured storage.
#[cfg(feature = "test-helpers")]
pub mod test {
    use crate as fuel_core_storage;
    use crate::kv_store::StorageColumn;
    use fuel_core_storage::{
        kv_store::{
            BatchOperations,
            KeyValueStore,
            Value,
        },
        Result as StorageResult,
    };
    use std::{
        cell::RefCell,
        collections::HashMap,
    };

    type Storage = RefCell<HashMap<(u32, Vec<u8>), Vec<u8>>>;

    /// The in-memory storage for testing purposes.
    #[derive(Debug, PartialEq, Eq)]
    pub struct InMemoryStorage<Column> {
        storage: Storage,
        _marker: core::marker::PhantomData<Column>,
    }

    impl<Column> Default for InMemoryStorage<Column> {
        fn default() -> Self {
            Self {
                storage: Storage::default(),
                _marker: Default::default(),
            }
        }
    }

    impl<Column> KeyValueStore for InMemoryStorage<Column>
    where
        Column: StorageColumn,
    {
        type Column = Column;

        fn write(
            &self,
            key: &[u8],
            column: Self::Column,
            buf: &[u8],
        ) -> StorageResult<usize> {
            let write = buf.len();
            self.storage
                .borrow_mut()
                .insert((column.id(), key.to_vec()), buf.to_vec());
            Ok(write)
        }

        fn delete(&self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            self.storage
                .borrow_mut()
                .remove(&(column.id(), key.to_vec()));
            Ok(())
        }

        fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
            Ok(self
                .storage
                .borrow_mut()
                .get(&(column.id(), key.to_vec()))
                .map(|v| v.clone().into()))
        }
    }

    impl<Column> BatchOperations for InMemoryStorage<Column> where Column: StorageColumn {}
}
