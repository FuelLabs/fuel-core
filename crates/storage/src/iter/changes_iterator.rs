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
    transactional::StorageChanges,
};

/// A type that allows to iterate over the `StorageChanges`.
pub struct ChangesIterator<'a, Column> {
    changes: &'a StorageChanges,
    _marker: core::marker::PhantomData<Column>,
}

impl<'a, Description> ChangesIterator<'a, Description> {
    /// Creates a new instance of the `ChangesIterator`.
    pub fn new(changes: &'a StorageChanges) -> Self {
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
        match self.changes {
            StorageChanges::Changes(changes) => Ok(changes
                .get(&column.id())
                .and_then(|tree| tree.get(key))
                .and_then(|operation| match operation {
                    WriteOperation::Insert(value) => Some(value.clone()),
                    WriteOperation::Remove => None,
                })),
            StorageChanges::ChangesList(changes_list) => {
                for changes in changes_list.iter() {
                    if let Some(value) = changes
                        .get(&column.id())
                        .and_then(|tree| tree.get(key))
                        .and_then(|operation| match operation {
                            WriteOperation::Insert(value) => Some(value.clone()),
                            WriteOperation::Remove => None,
                        })
                    {
                        return Ok(Some(value));
                    }
                }
                Ok(None)
            }
        }
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
        match self.changes {
            StorageChanges::Changes(changes) => {
                if let Some(tree) = changes.get(&column.id()) {
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
            StorageChanges::ChangesList(changes_list) => {
                // We have to clone the prefix and start, because we need to pass them to the iterator
                // if someone finds a solution without making it a vec, feel free to contribute :)
                let column = column.id();
                let prefix = prefix.map(|prefix| prefix.to_vec());
                let start = start.map(|start| start.to_vec());
                changes_list
                    .iter()
                    .filter_map(move |changes| {
                        changes.get(&column).map(|tree| {
                            crate::iter::iterator(
                                tree,
                                prefix.as_deref(),
                                start.as_deref(),
                                direction,
                            )
                            .filter_map(|(key, value)| match value {
                                WriteOperation::Insert(value) => {
                                    Some((key.clone().into(), value.clone()))
                                }
                                WriteOperation::Remove => None,
                            })
                            .map(Ok)
                        })
                    })
                    .flatten()
                    .into_boxed()
            }
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
        match self.changes {
            StorageChanges::Changes(changes) => {
                if let Some(tree) = changes.get(&column.id()) {
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
            StorageChanges::ChangesList(changes_list) => {
                // We have to clone the prefix and start, because we need to pass them to the iterator
                // if someone finds a solution without making it a vec, feel free to contribute :)
                let column = column.id();
                let prefix = prefix.map(|prefix| prefix.to_vec());
                let start = start.map(|start| start.to_vec());
                changes_list
                    .iter()
                    .filter_map(move |changes| {
                        changes.get(&column).map(|tree| {
                            crate::iter::iterator(
                                tree,
                                prefix.as_deref(),
                                start.as_deref(),
                                direction,
                            )
                            .filter_map(|(key, value)| match value {
                                WriteOperation::Insert(_) => Some(key.clone().into()),
                                WriteOperation::Remove => None,
                            })
                            .map(Ok)
                        })
                    })
                    .flatten()
                    .into_boxed()
            }
        }
    }
}
