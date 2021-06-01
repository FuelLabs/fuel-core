use crate::state::in_memory::transaction::MemoryTransactionView;
use crate::state::{BatchOperations, KeyValueStore, Transactional, WriteOperation};
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

impl<K, V> KeyValueStore for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    type Key = K;
    type Value = V;

    fn get(&self, key: Self::Key) -> Option<Self::Value> {
        self.inner
            .read()
            .expect("poisoned")
            .get(key.as_ref())
            .map(|v| bincode::deserialize(v).unwrap())
    }

    fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
        self.inner
            .write()
            .expect("poisoned")
            .insert(key.into(), bincode::serialize(&value).unwrap())
            .map(|res| bincode::deserialize(&res).unwrap())
    }

    fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
        self.inner
            .write()
            .expect("poisoned")
            .remove(key.as_ref())
            .map(|res| bincode::deserialize(&res).unwrap())
    }

    fn exists(&self, key: Self::Key) -> bool {
        self.inner.read().expect("poisoned").contains_key(key.as_ref())
    }
}

impl<K, V> BatchOperations for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    type Key = K;
    type Value = V;

    fn batch_write<I>(&mut self, entries: I)
    where
        I: Iterator<Item = WriteOperation<Self::Key, Self::Value>>,
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
    }
}

/// Configure memory store to use the MemoryTransactionView
impl<K, V> Transactional<K, V> for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
    type View = MemoryTransactionView<K, V, Self>;
}
