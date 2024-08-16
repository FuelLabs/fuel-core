use crate::{
    database::database_description::{
        on_chain::OnChain,
        DatabaseDescription,
    },
    state::{
        in_memory::memory_view::MemoryView,
        iterable_key_value_view::IterableKeyValueViewWrapper,
        IterDirection,
        IterableKeyValueView,
        KeyValueView,
        TransactableStorage,
    },
};
use fuel_core_storage::{
    iter::{
        iterator,
        keys_iterator,
        BoxedIter,
        IntoBoxedIter,
        IterableStore,
    },
    kv_store::{
        KVItem,
        KeyItem,
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::{
        Changes,
        ReferenceBytesKey,
    },
    Result as StorageResult,
};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    ops::Deref,
    sync::Mutex,
};

#[derive(Debug)]
pub struct MemoryStore<Description = OnChain>
where
    Description: DatabaseDescription,
{
    inner: Vec<Mutex<BTreeMap<ReferenceBytesKey, Value>>>,
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
    fn create_view(&self) -> MemoryView<Description> {
        // Lock all tables at the same time to have consistent view.
        let locks = self
            .inner
            .iter()
            .map(|lock| lock.lock().expect("Poisoned lock"))
            .collect::<Vec<_>>();
        let inner = locks
            .iter()
            .map(|btree| btree.deref().clone())
            .collect::<Vec<_>>();
        MemoryView {
            inner,
            _marker: Default::default(),
        }
    }

    pub fn iter_all(
        &self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KVItem> {
        let lock = self.inner[column.as_usize()].lock().expect("poisoned");

        let collection: Vec<_> = iterator(&lock, prefix, start, direction)
            .map(|(key, value)| (key.clone().into(), value.clone()))
            .collect();

        collection.into_iter().map(Ok)
    }

    pub fn iter_all_keys(
        &self,
        column: Description::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> impl Iterator<Item = KeyItem> {
        let lock = self.inner[column.as_usize()].lock().expect("poisoned");

        let collection: Vec<_> = keys_iterator(&lock, prefix, start, direction)
            .map(|key| key.to_vec())
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
            .get(key)
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

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<fuel_core_storage::kv_store::KeyItem> {
        self.iter_all_keys(column, prefix, start, direction)
            .into_boxed()
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

    fn view_at_height(
        &self,
        _: &Description::Height,
    ) -> StorageResult<KeyValueView<Self::Column>> {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1995
        Err(
            anyhow::anyhow!("The historical view is not implemented for `MemoryStore`")
                .into(),
        )
    }

    fn latest_view(&self) -> StorageResult<IterableKeyValueView<Self::Column>> {
        let view = self.create_view();
        Ok(IterableKeyValueView::from_storage(
            IterableKeyValueViewWrapper::new(view),
        ))
    }

    fn rollback_block_to(&self, _: &Description::Height) -> StorageResult<()> {
        // TODO: https://github.com/FuelLabs/fuel-core/issues/1995
        Err(
            anyhow::anyhow!("The historical view is not implemented for `MemoryStore`")
                .into(),
        )
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
            self.commit_changes(None, changes)?;
            Ok(len)
        }

        fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            let mut transaction = self.read_transaction();
            transaction.delete(key, column)?;
            let changes = transaction.into_changes();
            self.commit_changes(None, changes)?;
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
