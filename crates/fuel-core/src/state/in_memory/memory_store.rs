use crate::{
    database::database_description::{
        on_chain::OnChain,
        DatabaseDescription,
    },
    state::{
        IterDirection,
        TransactableStorage,
    },
};
use fuel_core_storage::{
    iter::{
        iterator,
        BoxedIter,
        IntoBoxedIter,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::Changes,
    Result as StorageResult,
};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    sync::Mutex,
};

#[derive(Debug)]
pub struct MemoryStore<Description = OnChain>
where
    Description: DatabaseDescription,
{
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

        let collection: Vec<_> = iterator(&lock, prefix, start, direction)
            .map(clone)
            .collect();

        collection.into_iter().map(Ok)
    }
}

impl<Description> KeyValueInspect for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    type Column = Description::Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        Ok(self.inner[column.as_usize()]
            .lock()
            .map_err(|e| anyhow::anyhow!("The lock is poisoned: {}", e))?
            .get(&key.to_vec())
            .cloned())
    }
}

impl<Description> IterableStore for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.iter_all(column, prefix, start, direction).into_boxed()
    }
}

impl<Description> TransactableStorage<Description::Height> for MemoryStore<Description>
where
    Description: DatabaseDescription,
{
    fn commit_changes(
        &self,
        _: Option<Description::Height>,
        changes: Changes,
    ) -> StorageResult<()> {
        for (column, btree) in changes.into_iter() {
            let mut lock = self.inner[column as usize]
                .lock()
                .map_err(|e| anyhow::anyhow!("The lock is poisoned: {}", e))?;

            for (key, operation) in btree.into_iter() {
                match operation {
                    WriteOperation::Insert(value) => {
                        lock.insert(key, value);
                    }
                    WriteOperation::Remove => {
                        lock.remove(&key);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        column::Column,
        kv_store::KeyValueMutate,
        transactional::ReadTransaction,
    };
    use std::sync::Arc;

    impl<Description> KeyValueMutate for MemoryStore<Description>
    where
        Description: DatabaseDescription,
    {
        fn write(
            &mut self,
            key: &[u8],
            column: Self::Column,
            buf: &[u8],
        ) -> StorageResult<usize> {
            let mut transaction = self.read_transaction();
            let len = transaction.write(key, column, buf)?;
            let changes = transaction.into_changes();
            self.commit_changes(Default::default(), changes)?;
            Ok(len)
        }

        fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            let mut transaction = self.read_transaction();
            transaction.delete(key, column)?;
            let changes = transaction.into_changes();
            self.commit_changes(Default::default(), changes)?;
            Ok(())
        }
    }

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let mut db = MemoryStore::<OnChain>::default();
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

        let mut db = MemoryStore::<OnChain>::default();
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

        let mut db = MemoryStore::<OnChain>::default();
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
