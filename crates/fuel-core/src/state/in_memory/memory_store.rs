use crate::{
    database::{
        Column,
        Error as DatabaseError,
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

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .get(&column_key(key, column))
            .map(|v| v.len()))
    }

    fn read(
        &self,
        key: &[u8],
        column: Column,
        mut buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        self.inner
            .lock()
            .expect("poisoned")
            .get(&column_key(key, column))
            .map(|value| {
                let read = value.len();
                std::io::Write::write_all(&mut buf, value.as_ref())
                    .map_err(|e| DatabaseError::Other(anyhow::anyhow!(e)))?;
                DatabaseResult::Ok(read)
            })
            .transpose()
    }

    fn read_alloc(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .get(&column_key(key, column))
            .map(|value| value.to_vec()))
    }

    fn write(&self, key: &[u8], column: Column, buf: Vec<u8>) -> DatabaseResult<usize> {
        let len = buf.len();
        self.inner
            .lock()
            .expect("poisoned")
            .insert(column_key(key, column), buf);
        Ok(len)
    }

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        buf: Vec<u8>,
    ) -> DatabaseResult<(usize, Option<Vec<u8>>)> {
        let len = buf.len();
        let existing = self
            .inner
            .lock()
            .expect("poisoned")
            .insert(column_key(key, column), buf);
        Ok((len, existing))
    }

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .remove(&column_key(key, column)))
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
