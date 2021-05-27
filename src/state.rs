use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::future::Future;

pub trait Database {}

#[async_trait::async_trait]
pub trait KeyValueStore {
    type Key: AsRef<[u8]> + Debug + Send + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    async fn get(&self, key: Self::Key) -> Option<Self::Value>;
    async fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value>;
    async fn delete(&mut self, key: Self::Key) -> Option<Self::Value>;
    async fn exists(&self, key: Self::Key) -> bool;
}

#[async_trait::async_trait]
pub trait BatchOperations {
    type Key: AsRef<[u8]> + Debug + Send + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    async fn batch_write<I>(&mut self, entries: I)
    where
        I: Iterator<Item = WriteOperation<Self::Key, Self::Value>> + Send;
}

#[derive(Debug)]
enum WriteOperation<K, V> {
    Insert(K, V),
    Remove(K),
}

#[async_trait::async_trait]
pub trait TransactionalProxy {
    type UnderlyingStore: BatchOperations;
    async fn commit(&mut self);
}

#[async_trait::async_trait]
pub trait Transactional {
    /// The proxy is used to provide a transactional view of state that merges
    /// uncommitted changes with the original source
    type StoreProxy: TransactionalProxy + Send + Sync;

    async fn transaction<F, Fut, S, R, E>(&mut self, f: F) -> TransactionResult<R, E>
    where
        F: FnOnce(&mut S) -> Fut + Send,
        Fut: Future<Output = TransactionResult<R, E>> + Send,
        S: KeyValueStore + Send;
}

pub type TransactionResult<T, E> = Result<T, TransactionError<E>>;

pub enum TransactionError<T> {
    Aborted(T),
}

pub mod in_memory {
    use crate::state::{KeyValueStore, TransactionalProxy, WriteOperation};
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::marker::PhantomData;
    use std::sync::{Arc, RwLock};

    pub mod memory_store {
        use crate::state::{BatchOperations, KeyValueStore, WriteOperation};
        use serde::de::DeserializeOwned;
        use serde::Serialize;
        use std::collections::HashMap;
        use std::fmt::Debug;
        use std::marker::PhantomData;
        use std::sync::{Arc, RwLock};

        #[derive(Clone, Debug)]
        pub struct MemoryStore<K, V> {
            inner: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
            _key_marker: PhantomData<K>,
            _value_marker: PhantomData<V>,
        }

        impl<K, V> MemoryStore<K, V> {
            pub fn new() -> Self {
                Self {
                    inner: Arc::new(RwLock::new(HashMap::<Vec<u8>, Vec<u8>>::new())),
                    _key_marker: PhantomData,
                    _value_marker: PhantomData,
                }
            }
        }

        #[async_trait::async_trait]
        impl<K, V> KeyValueStore for MemoryStore<K, V>
        where
            K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
            V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
        {
            type Key = K;
            type Value = V;

            async fn get(&self, key: Self::Key) -> Option<Self::Value> {
                self.inner
                    .read()
                    .expect("poisoned")
                    .get(key.as_ref())
                    .map(|v| bincode::deserialize(v).unwrap())
            }

            async fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
                self.inner
                    .write()
                    .expect("poisoned")
                    .insert(key.into(), bincode::serialize(&value).unwrap())
                    .map(|res| bincode::deserialize(&res).unwrap())
            }

            async fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
                self.inner
                    .write()
                    .expect("poisoned")
                    .remove(key.as_ref())
                    .map(|res| bincode::deserialize(&res).unwrap())
            }

            async fn exists(&self, key: Self::Key) -> bool {
                self.inner.read().expect("poisoned").contains_key(key.as_ref())
            }
        }

        #[async_trait::async_trait]
        impl<K, V> BatchOperations for MemoryStore<K, V>
        where
            K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
            V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
        {
            type Key = K;
            type Value = V;

            async fn batch_write<I>(&mut self, entries: I)
            where
                I: Iterator<Item = WriteOperation<Self::Key, Self::Value>> + Send,
            {
                for entry in entries {
                    match entry {
                        WriteOperation::Insert(key, value) => {
                            let _ = self.put(key, value).await;
                        }
                        WriteOperation::Remove(key) => {
                            let _ = self.delete(key).await;
                        }
                    }
                }
            }
        }
    }

    pub mod transaction_proxy {
        use crate::state::in_memory::memory_store::MemoryStore;
        use crate::state::{BatchOperations, KeyValueStore, TransactionalProxy, WriteOperation};
        use serde::de::DeserializeOwned;
        use serde::Serialize;
        use std::collections::HashMap;
        use std::fmt::Debug;

        struct MemoryTransactionProxy<K, V, S> {
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
                self.changes
                    .insert(key.clone().into(), WriteOperation::Remove(key.clone()));
                self.proxy_layer.delete(key).await
            }

            async fn exists(&self, key: Self::Key) -> bool {
                if self.changes.contains_key(key.as_ref()) {
                    self.proxy_layer.exists(key).await
                } else {
                    self.data_source.exists(key).await
                }
            }
        }
    }
}
