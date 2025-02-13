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
    transactional::{
        Changes,
        ReferenceBytesKey,
        StorageChanges,
    },
};
use alloc::collections::BTreeMap;

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
            StorageChanges::Changes(changes) => get_insert_keys_from_changes(
                changes,
                column.id(),
                prefix,
                start,
                direction,
            ),
            StorageChanges::ChangesList(changes_list) => {
                // We have to clone the prefix and start, because we need to pass them to the iterator
                // if someone finds a solution without making it a vec, feel free to contribute :)
                let column = column.id();
                let prefix = prefix.map(|prefix| prefix.to_vec());
                let start = start.map(|start| start.to_vec());
                changes_list
                    .iter()
                    .flat_map(move |changes| {
                        get_insert_keys_from_changes(
                            changes,
                            column,
                            prefix.as_deref(),
                            start.as_deref(),
                            direction,
                        )
                    })
                    .into_boxed()
            }
        }
    }
}

fn get_insert_keys_from_changes<'a>(
    changes: &'a Changes,
    column_id: u32,
    prefix: Option<&[u8]>,
    start: Option<&[u8]>,
    direction: IterDirection,
) -> BoxedIter<'a, crate::kv_store::KeyItem> {
    changes
        .get(&column_id)
        .map_or(core::iter::empty().into_boxed(), |tree| {
            boxed_insert_iter(tree, prefix, start, direction)
        })
}

fn boxed_insert_iter<'a>(
    tree: &'a BTreeMap<ReferenceBytesKey, WriteOperation>,
    prefix: Option<&[u8]>,
    start: Option<&[u8]>,
    direction: IterDirection,
) -> BoxedIter<'a, crate::kv_store::KeyItem> {
    crate::iter::iterator(tree, prefix, start, direction)
        .filter_map(|(key, value)| match value {
            WriteOperation::Insert(_) => Some(key.clone().into()),
            WriteOperation::Remove => None,
        })
        .map(Ok)
        .into_boxed()
}
