use crate::state::in_memory::column_key;
use crate::state::{
    in_memory::memory_store::MemoryStore, BatchOperations, ColumnId, DataSource, KeyValueStore,
    Result, TransactableStorage, Transaction, TransactionError, TransactionResult, WriteOperation,
};
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MemoryTransactionView {
    view_layer: MemoryStore,
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: Arc<Mutex<HashMap<Vec<u8>, WriteOperation>>>,
    data_source: DataSource,
}

impl MemoryTransactionView {
    pub fn new(source: DataSource) -> Self {
        Self {
            view_layer: MemoryStore::new(),
            changes: Default::default(),
            data_source: source,
        }
    }

    pub fn commit(&self) -> crate::state::Result<()> {
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
    fn get(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>> {
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

    fn put(&self, key: Vec<u8>, column: ColumnId, value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let k = column_key(&key, column);
        let contained_key = self.changes.lock().expect("poisoned lock").contains_key(&k);
        self.changes.lock().expect("poisoned lock").insert(
            k,
            WriteOperation::Insert(key.clone(), column, value.clone()),
        );
        let res = self.view_layer.put(key.clone(), column, value);
        if contained_key {
            res
        } else {
            self.data_source.get(&key, column)
        }
    }

    fn delete(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>> {
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

    fn exists(&self, key: &[u8], column: ColumnId) -> Result<bool> {
        let k = column_key(key, column);
        if self.changes.lock().expect("poisoned lock").contains_key(&k) {
            self.view_layer.exists(key, column)
        } else {
            self.data_source.exists(key, column)
        }
    }

    fn iter_all(&self, column: ColumnId) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_> {
        // iterate over inmemory + db while also filtering deleted entries
        let changes = self.changes.clone();
        Box::new(
            self.view_layer
                .iter_all(column)
                .chain(self.data_source.iter_all(column))
                .unique_by(|(key, _)| key.clone())
                .filter(move |(key, _)| {
                    if let Some(WriteOperation::Remove(_, _)) =
                        changes.lock().expect("poisoned").get(key.as_slice())
                    {
                        false
                    } else {
                        true
                    }
                }),
        )
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
        let store = Arc::new(MemoryStore::new());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        // test
        let ret = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        view.delete(&key, 0).unwrap();
        // test
        let ret = view.get(&key, 0).unwrap();
        let original = store.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some(vec![1, 2, 3]))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let view = MemoryTransactionView::new(store);
        let _ = view.put(vec![0xA, 0xB, 0xC], 0, vec![1, 2, 3]);
        // test
        let ret = view.put(vec![0xA, 0xB, 0xC], 0, vec![2, 4, 6]).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn delete_value_from_view_returns_value() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        // test
        let ret = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.delete(&key, 0).unwrap();
        let ret2 = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret1, Some(vec![1, 2, 3]));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let view = MemoryTransactionView::new(store);
        let key = vec![0xA, 0xB, 0xC];
        view.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        // test
        let ret = view.exists(&key, 0).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_checks_data_store_when_not_in_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.exists(&key, 0).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_doesnt_check_data_store_after_intentional_removal_from_view() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        view.delete(&key, 0).unwrap();
        // test
        let ret = view.exists(&key, 0).unwrap();
        let original = store.exists(&key, 0).unwrap();
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[test]
    fn commit_applies_puts() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let view = MemoryTransactionView::new(store.clone());
        let key = vec![0xA, 0xB, 0xC];
        view.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some(vec![1, 2, 3]))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let store = Arc::new(MemoryStore::new());
        let key = vec![0xA, 0xB, 0xC];
        store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
        let view = MemoryTransactionView::new(store.clone());
        // test
        view.delete(&key, 0).unwrap();
        view.commit().unwrap();
        let ret = store.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn transaction_commit_is_applied_if_successful() {
        let mut store = Arc::new(MemoryStore::new());

        let key = vec![0xA, 0xB, 0xC];
        store
            .transaction(|store| {
                store.put(key.clone(), 0, vec![1, 2, 3]).unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(store.get(&key, 0).unwrap().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn transaction_commit_is_not_applied_if_aborted() {
        let mut store = Arc::new(MemoryStore::new());

        let _result = store.transaction(|store| {
            store.put(vec![0xA, 0xB, 0xC], 0, vec![1, 2, 3]).unwrap();
            Err(TransactionError::Aborted)?;
            Ok(())
        });

        assert_eq!(store.get(&[0xA, 0xB, 0xC], 0).unwrap(), None);
    }
}
