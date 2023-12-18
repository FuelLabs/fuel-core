use crate::{
    database::{
        Column,
        Result as DatabaseResult,
    },
    state::{
        in_memory::memory_store::MemoryStore,
        BatchOperations,
        DataSource,
        IterDirection,
        KVItem,
        KeyValueStore,
        TransactableStorage,
        Value,
        WriteOperation,
    },
};
use fuel_core_storage::iter::{
    BoxedIter,
    IntoBoxedIter,
};
use itertools::{
    EitherOrBoth,
    Itertools,
};
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::Debug,
    ops::DerefMut,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Debug)]
pub struct MemoryTransactionView {
    view_layer: MemoryStore,
    // TODO: Remove `Mutex`.
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: [Mutex<HashMap<Vec<u8>, WriteOperation>>; Column::COUNT],
    data_source: DataSource,
}

impl MemoryTransactionView {
    pub fn new(source: DataSource) -> Self {
        Self {
            view_layer: MemoryStore::default(),
            changes: Default::default(),
            data_source: source,
        }
    }

    pub fn commit(&self) -> DatabaseResult<()> {
        let mut iter = self
            .changes
            .iter()
            .zip(enum_iterator::all::<Column>())
            .flat_map(|(column_map, column)| {
                let mut map = column_map.lock().expect("poisoned lock");
                let changes = core::mem::take(map.deref_mut());

                changes.into_iter().map(move |t| (t.0, column, t.1))
            });

        self.data_source.batch_write(&mut iter)
    }
}

impl KeyValueStore for MemoryTransactionView {
    type Column = Column;

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        value: Value,
    ) -> DatabaseResult<Option<Value>> {
        let key_vec = key.to_vec();
        let contained_key = self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .insert(key_vec, WriteOperation::Insert(value.clone()))
            .is_some();
        let res = self.view_layer.replace(key, column, value);
        if contained_key {
            res
        } else {
            self.data_source.get(key, column)
        }
    }

    fn write(&self, key: &[u8], column: Column, buf: &[u8]) -> DatabaseResult<usize> {
        let k = key.to_vec();
        self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .insert(k, WriteOperation::Insert(Arc::new(buf.to_vec())));
        self.view_layer.write(key, column, buf)
    }

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        let k = key.to_vec();
        let contained_key = {
            let mut lock = self.changes[column.as_usize()]
                .lock()
                .expect("poisoned lock");
            lock.insert(k, WriteOperation::Remove).is_some()
        };
        let res = self.view_layer.take(key, column);
        if contained_key {
            res
        } else {
            self.data_source.get(key, column)
        }
    }

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<()> {
        let k = key.to_vec();
        self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .insert(k, WriteOperation::Remove);
        self.view_layer.delete(key, column)
    }

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .contains_key(&key.to_vec())
        {
            self.view_layer.size_of_value(key, column)
        } else {
            // fall-through to original data source
            // Note: The getting size from original database may be more performant than from `get`
            self.data_source.size_of_value(key, column)
        }
    }

    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .contains_key(&key.to_vec())
        {
            self.view_layer.get(key, column)
        } else {
            // fall-through to original data source
            self.data_source.get(key, column)
        }
    }

    fn read(
        &self,
        key: &[u8],
        column: Column,
        buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes[column.as_usize()]
            .lock()
            .expect("poisoned lock")
            .contains_key(&key.to_vec())
        {
            self.view_layer.read(key, column, buf)
        } else {
            // fall-through to original data source
            // Note: The read from original database may be more performant than from `get`
            self.data_source.read(key, column, buf)
        }
    }

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        // iterate over inmemory + db while also filtering deleted entries
        self.view_layer
                // iter_all returns items in sorted order
                .iter_all(column, prefix, start, direction)
                // Merge two sorted iterators (our current view overlay + backing data source)
                .merge_join_by(
                    self.data_source.iter_all(column, prefix, start, direction),
                    move |i, j| {
                        if let (Ok(i), Ok(j)) = (i, j) {
                            if IterDirection::Forward == direction {
                                i.0.cmp(&j.0)
                            } else {
                                j.0.cmp(&i.0)
                            }
                        } else {
                            // prioritize errors from db result first
                            if j.is_err() {
                                Ordering::Greater
                            } else {
                                Ordering::Less
                            }
                        }
                    },
                )
                .map(|either_both| {
                    match either_both {
                        // in the case of overlap, choose the left-side (our view overlay)
                        EitherOrBoth::Both(v, _)
                        | EitherOrBoth::Left(v)
                        | EitherOrBoth::Right(v) => v,
                    }
                })
                // filter entries which have been deleted over the course of this transaction
                .filter(move |item| {
                    if let Ok((key, _)) = item {
                        !matches!(
                            self.changes[column.as_usize()]
                                .lock()
                                .expect("poisoned")
                                .get(key),
                            Some(WriteOperation::Remove)
                        )
                    } else {
                        // ensure errors are propagated
                        true
                    }
                }).into_boxed()
    }
}

impl BatchOperations for MemoryTransactionView {}

impl TransactableStorage for MemoryTransactionView {
    fn flush(&self) -> DatabaseResult<()> {
        for lock in self.changes.iter() {
            lock.lock().expect("poisoned lock").clear();
        }
        self.view_layer.flush()?;
        self.data_source.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn get_returns_from_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        view.put(&key, Column::Metadata, expected.clone()).unwrap();
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(expected))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected.clone()).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(expected))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected.clone()).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        view.delete(&key, Column::Metadata).unwrap();
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        let original = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some(expected))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let expected = Arc::new(vec![1, 2, 3]);
        view.put(&[0xA, 0xB, 0xC], Column::Metadata, expected.clone())
            .unwrap();
        // test
        let ret = view
            .replace(&[0xA, 0xB, 0xC], Column::Metadata, Arc::new(vec![2, 4, 6]))
            .unwrap();
        // verify
        assert_eq!(ret, Some(expected))
    }

    #[test]
    fn delete_value_from_view_returns_value() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        view.put(&key, Column::Metadata, expected.clone()).unwrap();
        // test
        let ret = view.take(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(expected));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected.clone()).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.take(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(expected));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected.clone()).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.take(&key, Column::Metadata).unwrap();
        let ret2 = view.take(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret1, Some(expected));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        view.put(&key, Column::Metadata, expected).unwrap();
        // test
        let ret = view.exists(&key, Column::Metadata).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_checks_data_store_when_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.exists(&key, Column::Metadata).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_does_not_check_data_store_after_intentional_removal_from_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        view.delete(&key, Column::Metadata).unwrap();
        // test
        let ret = view.exists(&key, Column::Metadata).unwrap();
        let original = store.exists(&key, Column::Metadata).unwrap();
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[test]
    fn commit_applies_puts() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store.clone());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        view.put(&key, Column::Metadata, expected.clone()).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(expected))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        let expected = Arc::new(vec![1, 2, 3]);
        store.put(&key, Column::Metadata, expected).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        // test
        view.delete(&key, Column::Metadata).unwrap();
        view.commit().unwrap();
        let ret = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn iter_all_is_sorted_across_source_and_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        (0..10).step_by(2).for_each(|i| {
            store
                .put(&[i], Column::Metadata, Arc::new(vec![1]))
                .unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(3).for_each(|i| {
            view.put(&[i], Column::Metadata, Arc::new(vec![2])).unwrap();
        });

        let ret: Vec<_> = view
            .iter_all(Column::Metadata, None, None, IterDirection::Forward)
            .map_ok(|(k, _)| k[0])
            .try_collect()
            .unwrap();
        // verify
        assert_eq!(ret, vec![0, 2, 3, 4, 6, 8, 9])
    }

    #[test]
    fn iter_all_is_reversible() {
        // setup
        let store = Arc::new(MemoryStore::default());
        (0..10).step_by(2).for_each(|i| {
            store
                .put(&[i], Column::Metadata, Arc::new(vec![1]))
                .unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(3).for_each(|i| {
            view.put(&[i], Column::Metadata, Arc::new(vec![2])).unwrap();
        });

        let ret: Vec<_> = view
            .iter_all(Column::Metadata, None, None, IterDirection::Reverse)
            .map_ok(|(k, _)| k[0])
            .try_collect()
            .unwrap();
        // verify
        assert_eq!(ret, vec![9, 8, 6, 4, 3, 2, 0])
    }

    #[test]
    fn iter_all_overrides_data_source_keys() {
        // setup
        let store = Arc::new(MemoryStore::default());
        (0..10).step_by(2).for_each(|i| {
            store
                .put(&[i], Column::Metadata, Arc::new(vec![0xA]))
                .unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(2).for_each(|i| {
            view.put(&[i], Column::Metadata, Arc::new(vec![0xB]))
                .unwrap();
        });

        let ret: Vec<_> = view
            .iter_all(Column::Metadata, None, None, IterDirection::Forward)
            // return all the values from the iterator
            .map_ok(|(_, v)| v[0])
            .try_collect()
            .unwrap();
        // verify
        assert_eq!(ret, vec![0xB, 0xB, 0xB, 0xB, 0xB])
    }

    #[test]
    fn iter_all_hides_deleted_data_source_keys() {
        // setup
        let store = Arc::new(MemoryStore::default());
        (0..10).step_by(2).for_each(|i| {
            store
                .put(&[i], Column::Metadata, Arc::new(vec![0xA]))
                .unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        view.delete(&[0], Column::Metadata).unwrap();
        view.delete(&[6], Column::Metadata).unwrap();

        let ret: Vec<_> = view
            .iter_all(Column::Metadata, None, None, IterDirection::Forward)
            // return all the values from the iterator
            .map_ok(|(k, _)| k[0])
            .try_collect()
            .unwrap();
        // verify
        assert_eq!(ret, vec![2, 4, 8])
    }

    #[test]
    fn can_use_unit_value() {
        let key = vec![0x00];

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );
    }

    #[test]
    fn can_use_unit_key() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );
    }

    #[test]
    fn can_use_unit_key_and_value() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, expected.clone()).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            expected
        );
    }
}
