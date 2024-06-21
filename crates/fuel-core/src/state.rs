use crate::database::database_description::DatabaseDescription;
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
use std::{
    fmt::Debug,
    sync::Arc,
};

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;

#[allow(type_alias_bounds)]
pub type DataSource<Description>
where
    Description: DatabaseDescription,
= Arc<dyn TransactableStorage<Description::Height, Column = Description::Column>>;

pub trait TransactableStorage<Height>: IterableStore + Debug + Send + Sync {
    /// Commits the changes into the storage.
    fn commit_changes(
        &self,
        height: Option<Height>,
        changes: Changes,
    ) -> StorageResult<()>;
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
}
