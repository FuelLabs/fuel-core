//! A type that allows to iterate over the `Changes`.

use crate::{
    Mappable,
    Result as StorageResult,
    blueprint::BlueprintCodec,
    codec::Decode,
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KVWriteItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    structured_storage::TableWithBlueprint,
    transactional::StorageChanges,
};

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

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

impl<Column> KeyValueInspect for ChangesIterator<'_, Column>
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
                let mut found_value = None;
                for changes in changes_list.iter() {
                    if let Some(value) = changes
                        .get(&column.id())
                        .and_then(|tree| tree.get(key))
                        .and_then(|operation| match operation {
                            WriteOperation::Insert(value) => Some(value.clone()),
                            WriteOperation::Remove => None,
                        })
                    {
                        if found_value.is_some() {
                            return Err(anyhow::anyhow!(
                                "Conflicting changes found for the column {} with {:?}",
                                column.name(),
                                key
                            )
                            .into());
                        }

                        found_value = Some(value);
                    }
                }
                Ok(found_value)
            }
        }
    }
}

impl<Column> IterableStore for ChangesIterator<'_, Column>
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
                let column = column.id();

                let mut iterators_list = Vec::with_capacity(changes_list.len());

                for changes in changes_list.iter() {
                    let iter = changes.get(&column).map(|tree| {
                        crate::iter::iterator(tree, prefix, start, direction)
                            .filter_map(|(key, value)| match value {
                                WriteOperation::Insert(value) => {
                                    Some((key.clone().into(), value.clone()))
                                }
                                WriteOperation::Remove => None,
                            })
                            .map(Ok)
                    });
                    if let Some(iter) = iter {
                        iterators_list.push(iter);
                    }
                }

                iterators_list.into_iter().flatten().into_boxed()
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
                let column = column.id();

                let mut iterators_list = Vec::with_capacity(changes_list.len());

                for changes in changes_list.iter() {
                    let iter = changes.get(&column).map(|tree| {
                        crate::iter::iterator(tree, prefix, start, direction)
                            .filter_map(|(key, value)| match value {
                                WriteOperation::Insert(_) => Some(key.clone().into()),
                                WriteOperation::Remove => None,
                            })
                            .map(Ok)
                    });
                    if let Some(iter) = iter {
                        iterators_list.push(iter);
                    }
                }

                iterators_list.into_iter().flatten().into_boxed()
            }
        }
    }
}

impl<Column> ChangesIterator<'_, Column>
where
    Column: StorageColumn,
{
    /// Returns an iterator over the all Key-Value modifications for the column.
    pub fn iter_store_writes(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVWriteItem> {
        match self.changes {
            StorageChanges::Changes(changes) => {
                if let Some(tree) = changes.get(&column.id()) {
                    crate::iter::iterator(tree, prefix, start, direction)
                        .map(|(key, value)| (key.clone().into(), value.clone()))
                        .map(Ok)
                        .into_boxed()
                } else {
                    core::iter::empty().into_boxed()
                }
            }
            StorageChanges::ChangesList(changes_list) => {
                let column = column.id();

                let mut iterators_list = Vec::with_capacity(changes_list.len());

                for changes in changes_list.iter() {
                    let iter = changes.get(&column).map(|tree| {
                        crate::iter::iterator(tree, prefix, start, direction)
                            .map(|(key, value)| (key.clone().into(), value.clone()))
                            .map(Ok)
                    });
                    if let Some(iter) = iter {
                        iterators_list.push(iter);
                    }
                }

                iterators_list.into_iter().flatten().into_boxed()
            }
        }
    }

    /// Returns an iterator over the all modifications in the table.
    #[allow(clippy::type_complexity)]
    pub fn iter_all_writes<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> BoxedIter<StorageResult<(M::OwnedKey, Option<M::OwnedValue>)>>
    where
        M: Mappable,
        M: TableWithBlueprint<Column = Column>,
        M::Blueprint: BlueprintCodec<M>,
    {
        self.iter_store_writes(M::column(), None, None, direction.unwrap_or_default())
            .map(|val| {
                val.and_then(|(key, value)| {
                    let key = <M::Blueprint as BlueprintCodec<M>>::KeyCodec::decode(
                        key.as_slice(),
                    )
                    .map_err(|e| crate::Error::Codec(anyhow::anyhow!(e)))?;
                    let value = match value {
                        WriteOperation::Insert(value) => Some(
                            <M::Blueprint as BlueprintCodec<M>>::ValueCodec::decode(
                                &value,
                            )
                            .map_err(|e| crate::Error::Codec(anyhow::anyhow!(e)))?,
                        ),
                        WriteOperation::Remove => None,
                    };
                    Ok((key, value))
                })
            })
            .into_boxed()
    }
}
