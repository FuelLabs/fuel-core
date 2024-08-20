use crate::{
    database::database_description::DatabaseDescription,
    state::{
        generic_database::GenericDatabase,
        iterable_key_value_view::IterableKeyValueViewWrapper,
        key_value_view::KeyValueViewWrapper,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::Changes,
    Result as StorageResult,
};
use std::fmt::Debug;

pub mod data_source;
pub mod generic_database;
#[cfg(feature = "rocksdb")]
pub mod historical_rocksdb;
pub mod in_memory;
pub mod iterable_key_value_view;
pub mod key_value_view;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
#[cfg(feature = "rocksdb")]
pub mod rocks_db_key_iterator;

pub type ColumnType<Description> = <Description as DatabaseDescription>::Column;

/// A type extends the `KeyValueView`, allowing iteration over the storage.
pub type IterableKeyValueView<Column> =
    GenericDatabase<IterableKeyValueViewWrapper<Column>>;

/// The basic view available for the key value storage.
pub type KeyValueView<Column> = GenericDatabase<KeyValueViewWrapper<Column>>;

impl<Column> IterableKeyValueView<Column>
where
    Column: StorageColumn + 'static,
{
    /// Downgrades the `IterableKeyValueView` into the `KeyValueView`.
    pub fn into_key_value_view(self) -> KeyValueView<Column> {
        let iterable = self.into_inner();
        let storage = KeyValueViewWrapper::new(iterable);
        KeyValueView::from_storage(storage)
    }
}

pub trait TransactableStorage<Height>: IterableStore + Debug + Send + Sync {
    /// Commits the changes into the storage.
    fn commit_changes(
        &self,
        height: Option<Height>,
        changes: Changes,
    ) -> StorageResult<()>;

    fn view_at_height(
        &self,
        height: &Height,
    ) -> StorageResult<KeyValueView<Self::Column>>;

    fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column>>;

    fn rollback_block_to(&self, height: &Height) -> StorageResult<()>;
}

// It is used only to allow conversion of the `StorageTransaction` into the `DataSource`.
#[cfg(feature = "test-helpers")]
impl<Height, S> TransactableStorage<Height>
    for fuel_core_storage::transactional::StorageTransaction<S>
where
    S: IterableStore + Debug + Send + Sync,
{
    fn commit_changes(&self, _: Option<Height>, _: Changes) -> StorageResult<()> {
        unimplemented!()
    }

    fn view_at_height(&self, _: &Height) -> StorageResult<KeyValueView<Self::Column>> {
        unimplemented!()
    }

    fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column>> {
        unimplemented!()
    }

    fn rollback_block_to(&self, _: &Height) -> StorageResult<()> {
        unimplemented!()
    }
}

/// A type that allows to iterate over the `Changes`.
pub struct ChangesIterator<'a, Description> {
    changes: &'a Changes,
    _marker: core::marker::PhantomData<Description>,
}

impl<'a, Description> ChangesIterator<'a, Description> {
    /// Creates a new instance of the `ChangesIterator`.
    pub fn new(changes: &'a Changes) -> Self {
        Self {
            changes,
            _marker: Default::default(),
        }
    }
}

impl<'a, Description> KeyValueInspect for ChangesIterator<'a, Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        Ok(self
            .changes
            .get(&column.id())
            .and_then(|tree| tree.get(key))
            .and_then(|operation| match operation {
                WriteOperation::Insert(value) => Some(value.clone()),
                WriteOperation::Remove => None,
            }))
    }
}

impl<'a, Description> IterableStore for ChangesIterator<'a, Description>
where
    Description: DatabaseDescription,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        if let Some(tree) = self.changes.get(&column.id()) {
            fuel_core_storage::iter::iterator(tree, prefix, start, direction)
                .filter_map(|(key, value)| match value {
                    WriteOperation::Insert(value) => {
                        Some((key.clone().into(), value.clone()))
                    }
                    WriteOperation::Remove => None,
                })
                .map(Ok)
                .into_boxed()
        } else {
            core::iter::empty().into_boxed()
        }
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        // We cannot define iter_store_keys appropriately for the `ChangesIterator`,
        // because we have to filter out the keys that were removed, which are
        // marked as `WriteOperation::Remove` in the value
        // copied as-is from the above function, but only to return keys
        if let Some(tree) = self.changes.get(&column.id()) {
            fuel_core_storage::iter::iterator(tree, prefix, start, direction)
                .filter_map(|(key, value)| match value {
                    WriteOperation::Insert(_) => Some(key.clone().into()),
                    WriteOperation::Remove => None,
                })
                .map(Ok)
                .into_boxed()
        } else {
            core::iter::empty().into_boxed()
        }
    }
}
