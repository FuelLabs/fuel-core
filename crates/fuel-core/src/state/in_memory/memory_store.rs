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
    inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
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
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> BoxedIter<DatabaseResult<(Vec<u8>, Vec<u8>)>> {
        // clone entire set so we can drop the lock
        let mut copy: Vec<(Vec<u8>, Vec<u8>)> = self
            .inner
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|(key, _)| is_column(key, column))
            // strip column
            .map(|(key, value)| (key[size_of::<ColumnId>()..].to_vec(), value.clone()))
            // filter prefix
            .filter(|(key, _)| {
                if let Some(prefix) = &prefix {
                    key.starts_with(prefix.as_slice())
                } else {
                    true
                }
            })
            .sorted()
            .collect();

        if direction == IterDirection::Reverse {
            copy.reverse();
        }

        if let Some(start) = start {
            copy.into_iter()
                .skip_while(move |(key, _)| key.as_slice() != start.as_slice())
                .map(Ok)
                .into_boxed()
        } else {
            copy.into_iter().map(Ok).into_boxed()
        }
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
