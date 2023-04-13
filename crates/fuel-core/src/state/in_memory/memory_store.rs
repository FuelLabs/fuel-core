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
    collections::{
        BTreeMap,
        HashMap,
    },
    fmt::Debug,
    mem::size_of,
    sync::{
        Mutex,
        MutexGuard,
        RwLockReadGuard,
    },
};

#[derive(Default, Debug)]
pub struct MemoryStore {
    // TODO: Remove `Mutex` and usage of the `column_key`.
    // TODO: Use `BTreeMap`.
    inner: Mutex<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryStore {
    pub fn iter_all<'a>(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KVItem> + 'a {
        let lock = self.inner.lock().expect("poisoned");

        // clone entire set so we can drop the lock
        let iter = lock
            .iter()
            .filter(|(key, _)| is_column(key, column))
            // strip column
            .map(|(key, value)| (&key[size_of::<ColumnId>()..], value.as_slice()))
            // filter prefix
            .filter(|(key, _)| {
                if let Some(prefix) = prefix {
                    key.starts_with(prefix)
                } else {
                    true
                }
            });

        let iter = if direction == IterDirection::Forward {
            iter.into_boxed()
        } else {
            iter.rev().into_boxed()
        };

        LockedIter {
            start: start.map(|b| b.to_vec()),
            direction,
            _guard: lock,
            inner: iter,
        }
    }
}

struct LockedIter<'a, D, I> {
    start: Option<Vec<u8>>,
    direction: IterDirection,
    _guard: MutexGuard<'a, D>,
    inner: I,
}

impl<'a, D, I: Iterator<Item = (&'a [u8], &'a [u8])>> Iterator for LockedIter<'a, D, I> {
    type Item = KVItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let until_start_reached = |&(key, _)| {
            if let Some(start) = &self.start {
                match self.direction {
                    IterDirection::Forward => key < start.as_slice(),
                    IterDirection::Reverse => key > start.as_slice(),
                }
            } else {
                false
            }
        };

        self.inner
            .skip_while(until_start_reached)
            .next()
            .map(DatabaseResult::Ok)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let db = MemoryStore::default();
        db.put(&key, Column::Metadata, vec![]).unwrap();

        assert_eq!(
            db.get(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), Vec::<u8>::with_capacity(0))]
        );

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let db = MemoryStore::default();
        db.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();

        assert_eq!(
            db.get(&key, Column::Metadata).unwrap().unwrap(),
            vec![1, 2, 3]
        );

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), vec![1, 2, 3])]
        );

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            vec![1, 2, 3]
        );

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key_and_value() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let db = MemoryStore::default();
        db.put(&key, Column::Metadata, vec![]).unwrap();

        assert_eq!(
            db.get(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), Vec::<u8>::with_capacity(0))]
        );

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }
}
