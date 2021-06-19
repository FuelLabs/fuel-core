use crate::state::{
    in_memory::memory_store::MemoryStore, BatchOperations, KeyValueStore, Result, Transaction,
    TransactionError, TransactionResult, WriteOperation,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;

pub struct MemoryTransactionView<K, V, S> {
    view_layer: MemoryStore<K, V>,
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: HashMap<Vec<u8>, WriteOperation<K, V>>,
    data_source: S,
}

impl<K, V, S> MemoryTransactionView<K, V, S>
where
    S: KeyValueStore<K, V> + BatchOperations<K, V>,
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    pub fn new(source: S) -> Self {
        Self {
            view_layer: MemoryStore::<K, V>::new(),
            changes: Default::default(),
            data_source: source,
        }
    }

    fn commit(&mut self) -> crate::state::Result<()> {
        self.data_source
            .batch_write(self.changes.drain().map(|t| t.1))
    }
}

impl<K, V, S> KeyValueStore<K, V> for MemoryTransactionView<K, V, S>
where
    S: KeyValueStore<K, V>,
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    fn get(&self, key: K) -> Result<Option<V>> {
        // try to fetch data from View layer if any changes to the key
        if self.changes.contains_key(key.as_ref()) {
            self.view_layer.get(key)
        } else {
            // fall-through to original data source
            self.data_source.get(key)
        }
    }

    fn put(&mut self, key: K, value: V) -> Result<Option<V>> {
        let contained_key = self.changes.contains_key(key.as_ref());
        self.changes.insert(
            key.clone().into(),
            WriteOperation::Insert(key.clone(), value.clone()),
        );
        let res = self.view_layer.put(key.clone(), value);
        if contained_key {
            res
        } else {
            self.data_source.get(key)
        }
    }

    fn delete(&mut self, key: K) -> Result<Option<V>> {
        let contained_key = self.changes.contains_key(key.as_ref());
        self.changes
            .insert(key.clone().into(), WriteOperation::Remove(key.clone()));
        let res = self.view_layer.delete(key.clone());
        if contained_key {
            res
        } else {
            self.data_source.get(key)
        }
    }

    fn exists(&self, key: K) -> Result<bool> {
        if self.changes.contains_key(key.as_ref()) {
            self.view_layer.exists(key)
        } else {
            self.data_source.exists(key)
        }
    }
}

impl<K, V, S> BatchOperations<K, V> for MemoryTransactionView<K, V, S>
where
    S: KeyValueStore<K, V>,
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    fn batch_write<I>(&mut self, entries: I) -> Result<()>
    where
        I: Iterator<Item = WriteOperation<K, V>>,
    {
        for entry in entries {
            match entry {
                WriteOperation::Insert(key, value) => {
                    let _ = self.put(key, value);
                }
                WriteOperation::Remove(key) => {
                    let _ = self.delete(key);
                }
            }
        }
        Ok(())
    }
}

impl<K, V, T> Transaction<K, V, MemoryTransactionView<K, V, T>> for T
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Into<Vec<u8>> + Serialize + DeserializeOwned + Debug + Clone,
    T: KeyValueStore<K, V> + BatchOperations<K, V> + Clone,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView<K, V, T>) -> TransactionResult<R>,
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
        let store = MemoryStore::<String, String>::new();
        let mut view = MemoryTransactionView::new(store);
        view.put("test".to_string(), "value".to_string()).unwrap();
        // test
        let ret = view.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_returns_from_data_store_when_key_not_in_view() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_view() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete("test".to_string()).unwrap();
        // test
        let ret = view.get("test".to_string()).unwrap();
        let original = store.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some("value".to_string()))
    }

    #[test]
    fn can_insert_value_into_view() {
        // setup
        let store = MemoryStore::<String, String>::new();
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
        let store = MemoryStore::<String, String>::new();
        let mut view = MemoryTransactionView::new(store);
        view.put("test".to_string(), "value".to_string()).unwrap();
        // test
        let ret = view.delete("test".to_string()).unwrap();
        let get = view.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_returns_datastore_value_when_not_in_view() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret = view.delete("test".to_string()).unwrap();
        let get = view.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[test]
    fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let mut view = MemoryTransactionView::new(store);
        // test
        let ret1 = view.delete("test".to_string()).unwrap();
        let ret2 = view.delete("test".to_string()).unwrap();
        let get = view.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret1, Some("value".to_string()));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[test]
    fn exists_checks_view_values() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut view = MemoryTransactionView::new(store);
        view.put("test".to_string(), "value".to_string()).unwrap();
        // test
        let ret = view.exists("test".to_string()).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_checks_data_store_when_not_in_view() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let view = MemoryTransactionView::new(store);
        // test
        let ret = view.exists("test".to_string()).unwrap();
        // verify
        assert!(ret)
    }

    #[test]
    fn exists_doesnt_check_data_store_after_intentional_removal_from_view() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        view.delete("test".to_string()).unwrap();
        // test
        let ret = view.exists("test".to_string()).unwrap();
        let original = store.exists("test".to_string()).unwrap();
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[test]
    fn commit_applies_puts() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut view = MemoryTransactionView::new(store.clone());
        view.put("test".to_string(), "value".to_string()).unwrap();
        // test
        view.commit().unwrap();
        let ret = store.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[test]
    fn commit_applies_deletes() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).unwrap();
        let mut view = MemoryTransactionView::new(store.clone());
        // test
        view.delete("test".to_string()).unwrap();
        view.commit().unwrap();
        let ret = store.get("test".to_string()).unwrap();
        // verify
        assert_eq!(ret, None)
    }

    #[test]
    fn transaction_commit_is_applied_if_successful() {
        let mut store = MemoryStore::<String, String>::new();

        store
            .transaction(|store| {
                store.put("k1".to_string(), "v1".to_string()).unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(store.get("k1".to_string()).unwrap().unwrap(), "v1")
    }

    #[test]
    fn transaction_commit_is_not_applied_if_aborted() {
        let mut store = MemoryStore::<String, String>::new();

        let _result = store.transaction(|store| {
            store.put("k1".to_string(), "v1".to_string()).unwrap();
            Err(TransactionError::Aborted)?;
            Ok(())
        });

        assert_eq!(store.get("k1".to_string()).unwrap(), None)
    }
}
