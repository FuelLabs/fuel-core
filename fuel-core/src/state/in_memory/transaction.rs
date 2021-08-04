use crate::state::in_memory::column_key;
use crate::state::{
    in_memory::memory_store::MemoryStore, BatchOperations, ColumnId, DataSource, KeyValueStore,
    Result, TransactableStorage, Transaction, TransactionError, TransactionResult, WriteOperation,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MemoryTransactionView<K, V> {
    view_layer: MemoryStore<K, V>,
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: HashMap<Vec<u8>, WriteOperation<K, V>>,
    data_source: DataSource<K, V>,
}

impl<K, V> MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    pub fn new(source: DataSource<K, V>) -> Self {
        Self {
            view_layer: MemoryStore::<K, V>::new(),
            changes: Default::default(),
            data_source: source,
        }
    }

    pub fn commit(&mut self) -> crate::state::Result<()> {
        self.data_source
            .lock()
            .expect("lock poisoned")
            .batch_write(&mut self.changes.drain().map(|t| t.1))
    }
}

impl<K, V> KeyValueStore<K, V> for MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
    fn get(&self, key: &K, column: ColumnId) -> Result<Option<V>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes.contains_key(&column_key(&key, column)) {
            self.view_layer.get(key, column)
        } else {
            // fall-through to original data source
            self.data_source
                .lock()
                .expect("lock poisoned")
                .get(key, column)
        }
    }

    fn put(&mut self, key: K, column: ColumnId, value: V) -> Result<Option<V>> {
        let k = column_key(&key, column);
        let contained_key = self.changes.contains_key(&k);
        self.changes.insert(
            k,
            WriteOperation::Insert(key.clone(), column, value.clone()),
        );
        let res = self.view_layer.put(key.clone(), column, value);
        if contained_key {
            res
        } else {
            self.data_source
                .lock()
                .expect("lock poisoned")
                .get(&key, column)
        }
    }

    fn delete(&mut self, key: &K, column: ColumnId) -> Result<Option<V>> {
        let k = column_key(&key.clone(), column);
        let contained_key = self.changes.contains_key(&k);
        self.changes
            .insert(k, WriteOperation::Remove(key.clone(), column));
        let res = self.view_layer.delete(key, column);
        if contained_key {
            res
        } else {
            self.data_source
                .lock()
                .expect("lock poisoned")
                .get(key, column)
        }
    }

    fn exists(&self, key: &K, column: ColumnId) -> Result<bool> {
        let k = column_key(&key.clone(), column);
        if self.changes.contains_key(&k) {
            self.view_layer.exists(key, column)
        } else {
            self.data_source
                .lock()
                .expect("lock poisoned")
                .exists(key, column)
        }
    }
}

impl<K, V> BatchOperations<K, V> for MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
}

impl<K, V, T> Transaction<K, V> for Arc<Mutex<T>>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Into<Vec<u8>> + Serialize + DeserializeOwned + Debug + Clone + Send,
    T: TransactableStorage<K, V> + 'static,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView<K, V>) -> TransactionResult<R>,
    {
        let mut view = MemoryTransactionView::new(self.clone());
        let result = f(&mut view);
        if result.is_ok() {
            view.commit().map_err(|_| TransactionError::Aborted)?;
        }
        result
    }
}

impl<K, V> TransactableStorage<K, V> for MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TransactionError;

    #[test]
    fn get_returns_from_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), 0, "value".to_string()).unwrap();
        // test
        let ret = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete(&key, 0).unwrap();
        // test
        let ret = view.get(&key, 0).unwrap();
        let original = store.lock().unwrap().get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some("value".to_string()))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let _ = view.put("test".to_string(), 0, "value".to_string());
        // test
        let ret = view
            .put("test".to_string(), 0, "value2".to_string())
            .unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn delete_value_from_view_returns_value() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), 0, "value".to_string()).unwrap();
        // test
        let ret = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.delete(&key, 0).unwrap();
        let ret2 = view.delete(&key, 0).unwrap();
        let get = view.get(&key, 0).unwrap();
        // verify
        assert_eq!(ret1, Some("value".to_string()));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), 0, "value".to_string()).unwrap();
        // test
        let ret = view.exists(&key, 0).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_checks_data_store_when_not_in_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.exists(&key, 0).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_doesnt_check_data_store_after_intentional_removal_from_view() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete(&key, 0).unwrap();
        // test
        let ret = view.exists(&key, 0).unwrap();
        let original = store.lock().unwrap().exists(&key, 0).unwrap();
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[test]
    fn commit_applies_puts() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store.clone());
        let key = "test".to_string();
        view.put(key.clone(), 0, "value".to_string()).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.lock().unwrap().get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .lock()
            .unwrap()
            .put(key.clone(), 0, "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        // test
        view.delete(&key, 0).unwrap();
        view.commit().unwrap();
        let ret = store.lock().unwrap().get(&key, 0).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn transaction_commit_is_applied_if_successful() {
        let mut store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));

        let key = "k1".to_string();
        store
            .transaction(|store| {
                store.put("k1".to_string(), 0, "v1".to_string()).unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(store.lock().unwrap().get(&key, 0).unwrap().unwrap(), "v1");
    }

    #[test]
    fn transaction_commit_is_not_applied_if_aborted() {
        let mut store = Arc::new(Mutex::new(MemoryStore::<String, String>::new()));

        let _result = store.transaction(|store| {
            store.put("k1".to_string(), 0, "v1".to_string()).unwrap();
            Err(TransactionError::Aborted)?;
            Ok(())
        });

        assert_eq!(
            store.lock().unwrap().get(&"k1".to_string(), 0).unwrap(),
            None
        );
    }
}
