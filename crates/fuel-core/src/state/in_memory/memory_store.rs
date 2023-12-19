use crate::{
    database::{
        Column,
        Result as DatabaseResult,
    },
    state::{
        BatchOperations,
        IterDirection,
        KVItem,
        KeyValueStore,
        TransactableStorage,
        Value,
    },
};
use fuel_core_storage::iter::{
    BoxedIter,
    IntoBoxedIter,
};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Default, Debug)]
pub struct MemoryStore {
    // TODO: Remove `Mutex`.
    inner: [Mutex<BTreeMap<Vec<u8>, Value>>; Column::COUNT],
}

impl MemoryStore {
    pub fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KVItem> {
        let lock = self.inner[column.as_usize()].lock().expect("poisoned");

        fn clone<K: Clone, V: Clone>(kv: (&K, &V)) -> (K, V) {
            (kv.0.clone(), kv.1.clone())
        }

        let collection: Vec<_> = match (prefix, start) {
            (None, None) => {
                if direction == IterDirection::Forward {
                    lock.iter().map(clone).collect()
                } else {
                    lock.iter().rev().map(clone).collect()
                }
            }
            (Some(prefix), None) => {
                if direction == IterDirection::Forward {
                    lock.range(prefix.to_vec()..)
                        .take_while(|(key, _)| key.starts_with(prefix))
                        .map(clone)
                        .collect()
                } else {
                    let mut vec: Vec<_> = lock
                        .range(prefix.to_vec()..)
                        .into_boxed()
                        .take_while(|(key, _)| key.starts_with(prefix))
                        .map(clone)
                        .collect();

                    vec.reverse();
                    vec
                }
            }
            (None, Some(start)) => {
                if direction == IterDirection::Forward {
                    lock.range(start.to_vec()..).map(clone).collect()
                } else {
                    lock.range(..=start.to_vec()).rev().map(clone).collect()
                }
            }
            (Some(prefix), Some(start)) => {
                if direction == IterDirection::Forward {
                    lock.range(start.to_vec()..)
                        .take_while(|(key, _)| key.starts_with(prefix))
                        .map(clone)
                        .collect()
                } else {
                    lock.range(..=start.to_vec())
                        .rev()
                        .take_while(|(key, _)| key.starts_with(prefix))
                        .map(clone)
                        .collect()
                }
            }
        };

        collection.into_iter().map(Ok)
    }
}

impl KeyValueStore for MemoryStore {
    type Column = Column;

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        value: Value,
    ) -> DatabaseResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .insert(key.to_vec(), value))
    }

    fn write(&self, key: &[u8], column: Column, buf: &[u8]) -> DatabaseResult<usize> {
        let len = buf.len();
        self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .insert(key.to_vec(), Arc::new(buf.to_vec()));
        Ok(len)
    }

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .remove(&key.to_vec()))
    }

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<()> {
        self.take(key, column).map(|_| ())
    }

    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .get(&key.to_vec())
            .cloned())
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

impl TransactableStorage for MemoryStore {
    fn flush(&self) -> DatabaseResult<()> {
        for lock in self.inner.iter() {
            lock.lock().expect("poisoned").clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let db = MemoryStore::default();
        let expected = Arc::new(vec![]);
        db.put(&key.to_vec(), Column::Metadata, expected.clone())
            .unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), expected.clone())]
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let db = MemoryStore::default();
        let expected = Arc::new(vec![1, 2, 3]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), expected.clone())]
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }

    #[test]
    fn can_use_unit_key_and_value() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let db = MemoryStore::default();
        let expected = Arc::new(vec![]);
        db.put(&key, Column::Metadata, expected.clone()).unwrap();

        assert_eq!(db.get(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(db.exists(&key, Column::Metadata).unwrap());

        assert_eq!(
            db.iter_all(Column::Metadata, None, None, IterDirection::Forward)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(key.clone(), expected.clone())]
        );

        assert_eq!(db.take(&key, Column::Metadata).unwrap().unwrap(), expected);

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }
}
