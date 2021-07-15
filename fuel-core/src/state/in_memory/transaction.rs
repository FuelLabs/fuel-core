use crate::state::{
    in_memory::memory_store::MemoryStore, BatchOperations, KeyValueStore, Result,
    TransactableStorage, Transaction, TransactionError, TransactionResult, WriteOperation,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

pub type DataSource<K, V> = Arc<RwLock<dyn TransactableStorage<K, V>>>;

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
            .write()
            .expect("lock poisoned")
            .batch_write(&mut self.changes.drain().map(|t| t.1))
    }
}

impl<K, V> KeyValueStore<K, V> for MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    fn get(&self, key: &K) -> Result<Option<V>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes.contains_key(key.as_ref()) {
            self.view_layer.get(key)
        } else {
            // fall-through to original data source
            self.data_source.read().expect("lock poisoned").get(key)
        }
    }

    fn put(&mut self, key: K, value: V) -> Result<Option<V>> {
        let contained_key = self.changes.contains_key(key.as_ref());
        self.changes.insert(
            key.as_ref().to_vec(),
            WriteOperation::Insert(key.clone(), value.clone()),
        );
        let res = self.view_layer.put(key.clone(), value);
        if contained_key {
            res
        } else {
            self.data_source.read().expect("lock poisoned").get(&key)
        }
    }

    fn delete(&mut self, key: &K) -> Result<Option<V>> {
        let contained_key = self.changes.contains_key(key.as_ref());
        self.changes
            .insert(key.as_ref().to_vec(), WriteOperation::Remove(key.clone()));
        let res = self.view_layer.delete(key);
        if contained_key {
            res
        } else {
            self.data_source.read().expect("lock poisoned").get(key)
        }
    }

    fn exists(&self, key: &K) -> Result<bool> {
        if self.changes.contains_key(key.as_ref()) {
            self.view_layer.exists(key)
        } else {
            self.data_source.read().expect("lock poisoned").exists(key)
        }
    }
}

impl<K, V> BatchOperations<K, V> for MemoryTransactionView<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
}

impl<K, V, T> Transaction<K, V> for Arc<RwLock<T>>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Into<Vec<u8>> + Serialize + DeserializeOwned + Debug + Clone,
    T: TransactableStorage<K, V> + 'static,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView<K, V>) -> TransactionResult<R>,
    {
        let mut view = MemoryTransactionView::new(self.clone());
        let result = f(&mut view);
        if let Ok(_) = result {
            view.commit().map_err(|_| TransactionError::Aborted)?;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TransactionError;

    #[test]
    fn get_returns_from_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), "value".to_string()).unwrap();
        // test
        let ret = view.get(&key).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get(&key).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete(&key).unwrap();
        // test
        let ret = view.get(&key).unwrap();
        let original = store.read().unwrap().get(&key).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some("value".to_string()))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let _ = view.put("test".to_string(), "value".to_string());
        // test
        let ret = view.put("test".to_string(), "value2".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn delete_value_from_view_returns_value() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), "value".to_string()).unwrap();
        // test
        let ret = view.delete(&key).unwrap();
        let get = view.get(&key).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret = view.delete(&key).unwrap();
        let get = view.get(&key).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.delete(&key).unwrap();
        let ret2 = view.delete(&key).unwrap();
        let get = view.get(&key).unwrap();
        // verify
        assert_eq!(ret1, Some("value".to_string()));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store);
        let key = "test".to_string();
        view.put(key.clone(), "value".to_string()).unwrap();
        // test
        let ret = view.exists(&key).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_checks_data_store_when_not_in_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.exists(&key).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_doesnt_check_data_store_after_intentional_removal_from_view() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete(&key).unwrap();
        // test
        let ret = view.exists(&key).unwrap();
        let original = store.read().unwrap().exists(&key).unwrap();
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[test]
    fn commit_applies_puts() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let mut view = MemoryTransactionView::new(store.clone());
        let key = "test".to_string();
        view.put(key.clone(), "value".to_string()).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.read().unwrap().get(&key).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));
        let key = "test".to_string();
        store
            .write()
            .unwrap()
            .put(key.clone(), "value".to_string())
            .unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        // test
        view.delete(&key).unwrap();
        view.commit().unwrap();
        let ret = store.read().unwrap().get(&key).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn transaction_commit_is_applied_if_successful() {
        let mut store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));

        let key = "k1".to_string();
        store
            .transaction(|store| {
                store.put("k1".to_string(), "v1".to_string()).unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(store.read().unwrap().get(&key).unwrap().unwrap(), "v1");
    }

    #[test]
    fn transaction_commit_is_not_applied_if_aborted() {
        let mut store = Arc::new(RwLock::new(MemoryStore::<String, String>::new()));

        let _result = store.transaction(|store| {
            store.put("k1".to_string(), "v1".to_string()).unwrap();
            Err(TransactionError::Aborted)?;
            Ok(())
        });

        assert_eq!(store.read().unwrap().get(&"k1".to_string()).unwrap(), None);
    }
}
