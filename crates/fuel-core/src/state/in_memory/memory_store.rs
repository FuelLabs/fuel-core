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
use itertools::Itertools;
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::Mutex,
};

#[derive(Default, Debug)]
pub struct MemoryStore {
    // TODO: Remove `Mutex`.
    // TODO: Use `BTreeMap`.
    inner: [Mutex<HashMap<Vec<u8>, Value>>; Column::COUNT],
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

        // clone entire set so we can drop the lock
        let iter = lock
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            // filter prefix
            .filter(|(key, _)| {
                if let Some(prefix) = prefix {
                    key.starts_with(prefix)
                } else {
                    true
                }
            })
            .sorted();

        let until_start_reached = |(key, _): &(Vec<u8>, Value)| {
            if let Some(start) = start {
                match direction {
                    IterDirection::Forward => key.as_slice() < start,
                    IterDirection::Reverse => key.as_slice() > start,
                }
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
    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .get(&key.to_vec())
            .cloned())
    }

    fn put(
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

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .remove(&key.to_vec()))
    }

    fn exists(&self, key: &[u8], column: Column) -> DatabaseResult<bool> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .contains_key(&key.to_vec()))
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

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );

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

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );

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

        assert_eq!(
            db.delete(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );

        assert!(!db.exists(&key, Column::Metadata).unwrap());
    }
}
