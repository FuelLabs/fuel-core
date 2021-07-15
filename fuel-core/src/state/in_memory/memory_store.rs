use crate::state::Error::Codec;
use crate::state::{BatchOperations, Error, KeyValueStore, Result, TransactableStorage};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

#[derive(Default, Debug)]
pub struct MemoryStore<K, V> {
    inner: HashMap<Vec<u8>, Vec<u8>>,
    _key_marker: PhantomData<K>,
    _value_marker: PhantomData<V>,
}

impl<K, V> MemoryStore<K, V> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::<Vec<u8>, Vec<u8>>::new(),
            _key_marker: PhantomData,
            _value_marker: PhantomData,
        }
    }
}

impl<K, V> KeyValueStore<K, V> for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
    fn get(&self, key: &K) -> Result<Option<V>> {
        if let Some(value) = self.inner.get(key.as_ref()) {
            Ok(Some(bincode::deserialize(value).map_err(|_| Codec)?))
        } else {
            Ok(None)
        }
    }

    fn put(&mut self, key: K, value: V) -> Result<Option<V>> {
        let value = bincode::serialize(&value).unwrap();
        let result = self.inner.insert(key.as_ref().to_vec(), value);
        if let Some(previous) = result {
            Ok(Some(
                bincode::deserialize(&previous).map_err(|_| Error::Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: &K) -> Result<Option<V>> {
        Ok(self
            .inner
            .remove(key.as_ref())
            .map(|res| bincode::deserialize(&res).unwrap()))
    }

    fn exists(&self, key: &K) -> Result<bool> {
        Ok(self.inner.contains_key(key.as_ref()))
    }
}

impl<K, V> BatchOperations<K, V> for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
}

impl<K, V> TransactableStorage<K, V> for MemoryStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
}
