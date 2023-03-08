use crate::{
    database::{
        Column,
        Result as DatabaseResult,
    },
    state::{
        in_memory::{
            column_key,
            memory_store::MemoryStore,
        },
        BatchOperations,
        DataSource,
        IterDirection,
        KeyValueStore,
        TransactableStorage,
        Transaction,
        TransactionError,
        TransactionResult,
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
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Debug)]
pub struct MemoryTransactionView {
    view_layer: MemoryStore,
    // TODO: Remove `Mutex` and usage of the `column_key`.
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: Arc<Mutex<HashMap<Vec<u8>, WriteOperation>>>,
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
        self.data_source.batch_write(
            &mut self
                .changes
                .lock()
                .expect("poisoned lock")
                .drain()
                .map(|t| t.1),
        )
    }
}

impl KeyValueStore for MemoryTransactionView {
    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        // try to fetch data from View layer if any changes to the key
        if self
            .changes
            .lock()
            .expect("poisoned lock")
            .contains_key(&column_key(key, column))
        {
            self.view_layer.get(key, column)
        } else {
            // fall-through to original data source
            self.data_source.get(key, column)
        }
    }

    fn put(
        &self,
        key: &[u8],
        column: Column,
        value: Vec<u8>,
    ) -> DatabaseResult<Option<Vec<u8>>> {
        let k = column_key(key, column);
        let contained_key = {
            let mut lock = self.changes.lock().expect("poisoned lock");
            let contained_key = lock.contains_key(&k);
            lock.insert(k, WriteOperation::Insert(key.into(), column, value.clone()));
            contained_key
        };
        let res = self.view_layer.put(key, column, value);
        if contained_key {
            res
        } else {
            self.data_source.get(key, column)
        }
    }

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        let k = column_key(key, column);
        let contained_key = self.changes.lock().expect("poisoned lock").contains_key(&k);
        self.changes
            .lock()
            .expect("poisoned lock")
            .insert(k, WriteOperation::Remove(key.to_vec(), column));
        let res = self.view_layer.delete(key, column);
        if contained_key {
            res
        } else {
            self.data_source.get(key, column)
        }
    }

    fn exists(&self, key: &[u8], column: Column) -> DatabaseResult<bool> {
        let k = column_key(key, column);
        if self.changes.lock().expect("poisoned lock").contains_key(&k) {
            self.view_layer.exists(key, column)
        } else {
            self.data_source.exists(key, column)
        }
    }

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<DatabaseResult<(Vec<u8>, Vec<u8>)>> {
        // iterate over inmemory + db while also filtering deleted entries
        let changes = self.changes.clone();

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
                            changes
                                .lock()
                                .expect("poisoned")
                                .get(&column_key(key, column)),
                            Some(WriteOperation::Remove(_, _))
                        )
                    } else {
                        // ensure errors are propagated
                        true
                    }
                }).into_boxed()
    }

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>> {
        // try to fetch data from View layer if any changes to the key
        if self
            .changes
            .lock()
            .expect("poisoned lock")
            .contains_key(&column_key(key, column))
        {
            self.view_layer.size_of_value(key, column)
        } else {
            // fall-through to original data source
            self.data_source.size_of_value(key, column)
        }
    }

    fn read(
        &self,
        key: &[u8],
        column: Column,
        buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>> {
        // try to fetch data from View layer if any changes to the key
        if self
            .changes
            .lock()
            .expect("poisoned lock")
            .contains_key(&column_key(key, column))
        {
            self.view_layer.read(key, column, buf)
        } else {
            // fall-through to original data source
            self.data_source.read(key, column, buf)
        }
    }

    fn read_alloc(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        if self
            .changes
            .lock()
            .expect("poisoned lock")
            .contains_key(&column_key(key, column))
        {
            self.view_layer.read_alloc(key, column)
        } else {
            // fall-through to original data source
            self.data_source.read_alloc(key, column)
        }
    }

    fn write(&self, key: &[u8], column: Column, buf: Vec<u8>) -> DatabaseResult<usize> {
        let k = column_key(key, column);
        self.changes
            .lock()
            .expect("poisoned lock")
            .insert(k, WriteOperation::Insert(key.into(), column, buf.clone()));
        self.view_layer.write(key, column, buf)
    }

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        buf: Vec<u8>,
    ) -> DatabaseResult<(usize, Option<Vec<u8>>)> {
        let k = column_key(key, column);
        let contained_key = {
            let mut lock = self.changes.lock().expect("poisoned lock");
            let contained_key = lock.contains_key(&k);
            lock.insert(k, WriteOperation::Insert(key.into(), column, buf.clone()));
            contained_key
        };
        let res = self.view_layer.replace(key, column, buf)?;
        let num_written = res.0;
        if contained_key {
            Ok(res)
        } else {
            Ok((num_written, self.data_source.read_alloc(key, column)?))
        }
    }

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>> {
        let k = column_key(key, column);
        let contained_key = {
            let mut lock = self.changes.lock().expect("poisoned lock");
            let contains_key = lock.contains_key(&k);
            lock.insert(k, WriteOperation::Remove(key.to_vec(), column));
            contains_key
        };
        let res = self.view_layer.take(key, column);
        if contained_key {
            res
        } else {
            self.data_source.read_alloc(key, column)
        }
    }
}

impl BatchOperations for MemoryTransactionView {}

impl<T> Transaction for Arc<T>
where
    T: TransactableStorage + 'static,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView) -> TransactionResult<R>,
    {
        let mut view = MemoryTransactionView::new(self.clone());
        let result = f(&mut view);
        if result.is_ok() {
            view.commit().map_err(|_| TransactionError::Aborted)?;
        }
        result
    }
}

impl TransactableStorage for MemoryTransactionView {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TransactionError;

    #[test]
    fn get_returns_from_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        view.delete(&key, Column::Metadata).unwrap();
        // test
        let ret = view.get(&key, Column::Metadata).unwrap();
        let original = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some(vec![1, 2, 3]))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let _ = view.put(&[0xA, 0xB, 0xC], Column::Metadata, vec![1, 2, 3]);
        // test
        let ret = view
            .put(&[0xA, 0xB, 0xC], Column::Metadata, vec![2, 4, 6])
            .unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn delete_value_from_view_returns_value() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        // test
        let ret = view.delete(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.delete(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.delete(&key, Column::Metadata).unwrap();
        let ret2 = view.delete(&key, Column::Metadata).unwrap();
        let get = view.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret1, Some(vec![1, 2, 3]));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
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
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
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
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
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
        view.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let store = Arc::new(MemoryStore::default());
        let key = vec![0xA, 0xB, 0xC];
        store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        // test
        view.delete(&key, Column::Metadata).unwrap();
        view.commit().unwrap();
        let ret = store.get(&key, Column::Metadata).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn transaction_commit_is_applied_if_successful() {
        let mut store = Arc::new(MemoryStore::default());

        let key = vec![0xA, 0xB, 0xC];
        store
            .transaction(|store| {
                store.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn transaction_commit_is_not_applied_if_aborted() {
        let mut store = Arc::new(MemoryStore::default());

        let _ = store.transaction(|store| {
            store
                .put(&[0xA, 0xB, 0xC], Column::Metadata, vec![1, 2, 3])
                .unwrap();
            TransactionResult::<()>::Err(TransactionError::Aborted)
        });

        assert_eq!(store.get(&[0xA, 0xB, 0xC], Column::Metadata).unwrap(), None);
    }

    #[test]
    fn iter_all_is_sorted_across_source_and_view() {
        // setup
        let store = Arc::new(MemoryStore::default());
        (0..10).step_by(2).for_each(|i| {
            store.put(&[i], Column::Metadata, vec![1]).unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(3).for_each(|i| {
            view.put(&[i], Column::Metadata, vec![2]).unwrap();
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
            store.put(&[i], Column::Metadata, vec![1]).unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(3).for_each(|i| {
            view.put(&[i], Column::Metadata, vec![2]).unwrap();
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
            store.put(&[i], Column::Metadata, vec![0xA]).unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        (0..10).step_by(2).for_each(|i| {
            view.put(&[i], Column::Metadata, vec![0xB]).unwrap();
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
            store.put(&[i], Column::Metadata, vec![0xA]).unwrap();
        });

        let view = MemoryTransactionView::new(store);
        // test
        let _ = view.delete(&[0], Column::Metadata).unwrap();
        let _ = view.delete(&[6], Column::Metadata).unwrap();

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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, vec![]).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );
    }

    #[test]
    fn can_use_unit_key() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, vec![1, 2, 3]).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn can_use_unit_key_and_value() {
        let key: Vec<u8> = Vec::with_capacity(0);

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
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

        db.commit().unwrap();

        assert!(!store.exists(&key, Column::Metadata).unwrap());

        let store = Arc::new(MemoryStore::default());
        let db = MemoryTransactionView::new(store.clone());
        db.put(&key, Column::Metadata, vec![]).unwrap();
        db.commit().unwrap();

        assert_eq!(
            store.get(&key, Column::Metadata).unwrap().unwrap(),
            Vec::<u8>::with_capacity(0)
        );
    }
}
