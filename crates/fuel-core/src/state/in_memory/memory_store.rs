use crate::{
    database::{
        Column,
        Result as DatabaseResult,
    },
    state::{
        in_memory::{
            column_key,
            is_column,
        },
        BatchOperations,
        ColumnId,
        IterDirection,
        KVItem,
        KeyValueStore,
        TransactableStorage,
    },
};
use fuel_core_storage::iter::{
    BoxedIter,
    IntoBoxedIter,
};
use itertools::Itertools;
use std::{
    collections::HashMap,
    fmt::Debug,
    mem::size_of,
    sync::Mutex,
};

#[derive(Default, Debug)]
pub struct MemoryStore {
    // TODO: Remove `Mutex` and usage of the `column_key`.
    // TODO: Use `BTreeMap`.
    inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryStore {
    pub fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KVItem> {
        let lock = self.inner.lock().expect("poisoned");

        // clone entire set so we can drop the lock
        let iter = lock
            .iter()
            .filter(|(key, _)| is_column(key, column))
            // strip column
            .map(|(key, value)| (key[size_of::<ColumnId>()..].to_vec(), value.clone()))
            // filter prefix
            .filter(|(key, _)| {
                if let Some(prefix) = prefix {
                    key.starts_with(prefix)
                } else {
                    true
                }
            })
            .sorted();

        let until_start_reached = |(key, _): &(Vec<u8>, Vec<u8>)| {
            if let Some(start) = start {
                key.as_slice() != start
            } else {
                false
            }
        };

        let copy: Vec<_> = match direction {
            IterDirection::Forward => iter.skip_while(until_start_reached).collect(),
            IterDirection::Reverse => {
                iter.rev().skip_while(until_start_reached).collect()
            }
        };

        copy.into_iter().map(Ok)
    }
}

impl KeyValueStore for MemoryStore {
    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .get(&column_key(key, column))
            .cloned())
    }

    fn put(
        &self,
        key: &[u8],
        column: Column,
        value: Vec<u8>,
    ) -> DatabaseResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .insert(column_key(key, column), value))
    }

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .remove(&column_key(key, column)))
    }

    fn exists(&self, key: &[u8], column: Column) -> DatabaseResult<bool> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .contains_key(&column_key(key, column)))
    }

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.iter_all(column, prefix, start, direction).into_boxed()
    }
}

impl BatchOperations for MemoryStore {}

impl TransactableStorage for MemoryStore {}
