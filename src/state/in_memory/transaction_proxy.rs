use crate::state::in_memory::memory_store::MemoryStore;
use crate::state::{BatchOperations, KeyValueStore, TransactionalProxy, WriteOperation};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;

pub struct MemoryTransactionProxy<K, V, S> {
    proxy_layer: MemoryStore<K, V>,
    // use hashmap to collapse changes (e.g. insert then remove the same key)
    changes: HashMap<Vec<u8>, WriteOperation<K, V>>,
    data_source: S,
}

impl<K, V, S> MemoryTransactionProxy<K, V, S>
where
    S: KeyValueStore<Key = K, Value = V> + BatchOperations<Key = K, Value = V>,
    K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
    V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
{
    pub fn new(source: S) -> Self {
        Self {
            proxy_layer: MemoryStore::<K, V>::new(),
            changes: Default::default(),
            data_source: source,
        }
    }
}

#[async_trait::async_trait]
impl<K, V, S> TransactionalProxy for MemoryTransactionProxy<K, V, S>
where
    S: KeyValueStore<Key = K, Value = V> + BatchOperations<Key = K, Value = V> + Send,
    K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
    V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
{
    type UnderlyingStore = S;

    async fn commit(&mut self) {
        self.data_source.batch_write(self.changes.drain().map(|t| t.1)).await
    }
}

#[async_trait::async_trait]
impl<K, V, S> KeyValueStore for MemoryTransactionProxy<K, V, S>
where
    S: KeyValueStore<Key = K, Value = V> + Send + Sync,
    K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
    V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
{
    type Key = K;
    type Value = V;

    async fn get(&self, key: Self::Key) -> Option<Self::Value> {
        // try to fetch data from proxy layer if any changes to the key
        if self.changes.contains_key(key.as_ref()) {
            self.proxy_layer.get(key).await
        } else {
            // fall-through to original data source
            self.data_source.get(key).await
        }
    }

    async fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
        self.changes
            .insert(key.clone().into(), WriteOperation::Insert(key.clone(), value.clone()));
        self.proxy_layer.put(key, value).await
    }

    async fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
        let contained_key = self.changes.contains_key(key.as_ref());
        self.changes
            .insert(key.clone().into(), WriteOperation::Remove(key.clone()));
        let res = self.proxy_layer.delete(key.clone()).await;
        if contained_key {
            res
        } else {
            self.data_source.get(key).await
        }
    }

    async fn exists(&self, key: Self::Key) -> bool {
        if self.changes.contains_key(key.as_ref()) {
            self.proxy_layer.exists(key).await
        } else {
            self.data_source.exists(key).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_returns_from_proxy() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut proxy = MemoryTransactionProxy::new(store);
        proxy.put("test".to_string(), "value".to_string()).await;
        // test
        let ret = proxy.get("test".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[tokio::test]
    async fn get_returns_from_data_store_when_key_not_in_proxy() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store);
        // test
        let ret = proxy.get("test".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[tokio::test]
    async fn get_does_not_fetch_from_datastore_if_intentionally_deleted_from_proxy() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store.clone());
        proxy.delete("test".to_string()).await;
        // test
        let ret = proxy.get("test".to_string()).await;
        let original = store.get("test".to_string()).await;
        // verify
        assert_eq!(ret, None);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert_eq!(original, Some("value".to_string()))
    }

    #[tokio::test]
    async fn can_insert_value_into_proxy() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut proxy = MemoryTransactionProxy::new(store);
        proxy.put("test".to_string(), "value".to_string()).await;
        // test
        let ret = proxy.put("test".to_string(), "value2".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[tokio::test]
    async fn delete_value_from_proxy_returns_value() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut proxy = MemoryTransactionProxy::new(store);
        proxy.put("test".to_string(), "value".to_string()).await;
        // test
        let ret = proxy.delete("test".to_string()).await;
        let get = proxy.get("test".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[tokio::test]
    async fn delete_returns_datastore_value_when_not_in_proxy() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store);
        // test
        let ret = proxy.delete("test".to_string()).await;
        let get = proxy.get("test".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()));
        assert_eq!(get, None)
    }

    #[tokio::test]
    async fn delete_does_not_return_datastore_value_when_deleted_twice() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store);
        // test
        let ret1 = proxy.delete("test".to_string()).await;
        let ret2 = proxy.delete("test".to_string()).await;
        let get = proxy.get("test".to_string()).await;
        // verify
        assert_eq!(ret1, Some("value".to_string()));
        assert_eq!(ret2, None);
        assert_eq!(get, None)
    }

    #[tokio::test]
    async fn exists_checks_proxy_values() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut proxy = MemoryTransactionProxy::new(store);
        proxy.put("test".to_string(), "value".to_string()).await;
        // test
        let ret = proxy.exists("test".to_string()).await;
        // verify
        assert!(ret)
    }

    #[tokio::test]
    async fn exists_checks_data_store_when_not_in_proxy() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store);
        // test
        let ret = proxy.exists("test".to_string()).await;
        // verify
        assert!(ret)
    }

    #[tokio::test]
    async fn exists_doesnt_check_data_store_after_intentional_removal_from_proxy() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store.clone());
        proxy.delete("test".to_string()).await;
        // test
        let ret = proxy.exists("test".to_string()).await;
        let original = store.exists("test".to_string()).await;
        // verify
        assert!(!ret);
        // also ensure the original value is still intact and we aren't just passing
        // through None from the data store
        assert!(original)
    }

    #[tokio::test]
    async fn commit_applies_puts() {
        // setup
        let store = MemoryStore::<String, String>::new();
        let mut proxy = MemoryTransactionProxy::new(store.clone());
        proxy.put("test".to_string(), "value".to_string()).await;
        // test
        proxy.commit().await;
        let ret = store.get("test".to_string()).await;
        // verify
        assert_eq!(ret, Some("value".to_string()))
    }

    #[tokio::test]
    async fn commit_applies_deletes() {
        // setup
        let mut store = MemoryStore::<String, String>::new();
        store.put("test".to_string(), "value".to_string()).await;
        let mut proxy = MemoryTransactionProxy::new(store.clone());
        // test
        proxy.delete("test".to_string()).await;
        proxy.commit().await;
        let ret = store.get("test".to_string()).await;
        // verify
        assert_eq!(ret, None)
    }
}
