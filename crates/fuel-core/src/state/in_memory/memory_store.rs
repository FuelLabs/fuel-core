use crate::{
    database::{
        database_description::{
            on_chain::OnChain,
            DatabaseDescription,
        },
        Result as DatabaseResult,
    },
    state::{
        BatchOperations,
        IterDirection,
        TransactableStorage,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IteratorableStore,
    },
    kv_store::{
        KVItem,
        KeyValueStore,
        StorageColumn,
        Value,
    },
    Result as StorageResult,
};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Debug)]
pub struct MemoryStore<Description = OnChain>
where
    Description: DatabaseDescription,
{
    // TODO: Remove `Mutex`.
    inner: Vec<Mutex<BTreeMap<Vec<u8>, Value>>>,
    _marker: core::marker::PhantomData<Description>,
}

impl<Description> Default for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    fn default() -> Self {
        use strum::EnumCount;
        Self {
            inner: (0..Description::Column::COUNT)
                .map(|_| Mutex::new(BTreeMap::new()))
                .collect(),
            _marker: Default::default(),
        }
    }
}

impl<Description> MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    pub fn iter_all(
        &self,
        column: Description::Column,
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

impl<Description> KeyValueStore for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn replace(
        &self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .insert(key.to_vec(), value))
    }

    fn write(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        let len = buf.len();
        self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .insert(key.to_vec(), Arc::new(buf.to_vec()));
        Ok(len)
    }

    fn take(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .remove(&key.to_vec()))
    }

    fn delete(&self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        self.take(key, column).map(|_| ())
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .expect("poisoned")
            .get(&key.to_vec())
            .cloned())
    }
}

impl<Description> IteratorableStore for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    fn iter_all(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.iter_all(column, prefix, start, direction).into_boxed()
    }
}

impl<Description> BatchOperations for MemoryStore<Description> where
    Description: DatabaseDescription
{
}

impl<Description> TransactableStorage for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
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
    use fuel_core_storage::column::Column;
    use std::sync::Arc;

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let db = MemoryStore::<OnChain>::default();
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

        let db = MemoryStore::<OnChain>::default();
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

        let db = MemoryStore::<OnChain>::default();
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
