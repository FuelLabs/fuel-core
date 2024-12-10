//! A type that allows to iterate over the `Changes`.

use crate::{
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
};

/// A type that allows to iterate over the `Changes`.
pub struct ChangesIterator<'a, Column> {
    changes: &'a Changes,
    _marker: core::marker::PhantomData<Column>,
}

impl<'a, Column> ChangesIterator<'a, Column> {
    /// Creates a new instance of the `ChangesIterator`.
    pub fn new(changes: &'a Changes) -> Self {
        Self {
            changes,
            _marker: Default::default(),
        }
    }
}

impl<'a, Column> KeyValueInspect for ChangesIterator<'a, Column>
where
    Column: StorageColumn,
{
    type Column = Column;

    fn get(&self, key: &[u8], column: Self::Column) -> crate::Result<Option<Value>> {
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

impl<'a, Column> IterableStore for ChangesIterator<'a, Column>
where
    Column: StorageColumn,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        if let Some(tree) = self.changes.get(&column.id()) {
            crate::iter::iterator(tree, prefix, start, direction)
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
    ) -> BoxedIter<crate::kv_store::KeyItem> {
        // We cannot define iter_store_keys appropriately for the `ChangesIterator`,
        // because we have to filter out the keys that were removed, which are
        // marked as `WriteOperation::Remove` in the value
        // copied as-is from the above function, but only to return keys
        if let Some(tree) = self.changes.get(&column.id()) {
            crate::iter::iterator(tree, prefix, start, direction)
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
