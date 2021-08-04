use crate::state::in_memory::column_key;
use crate::state::Error::Codec;
use crate::state::{BatchOperations, ColumnId, Error, KeyValueStore, Result, TransactableStorage};
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
    fn get(&self, key: &K, column: ColumnId) -> Result<Option<V>> {
        if let Some(value) = self.inner.get(&column_key(key, column)) {
            Ok(Some(bincode::deserialize(value).map_err(|_| Codec)?))
        } else {
            Ok(None)
        }
    }

    fn put(&mut self, key: K, column: ColumnId, value: V) -> Result<Option<V>> {
        let value = bincode::serialize(&value).unwrap();
        let result = self.inner.insert(column_key(&key, column), value);
        if let Some(previous) = result {
            Ok(Some(
                bincode::deserialize(&previous).map_err(|_| Error::Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: &K, column: ColumnId) -> Result<Option<V>> {
        Ok(self
            .inner
            .remove(&column_key(key, column))
            .map(|res| bincode::deserialize(&res).unwrap()))
    }

    fn exists(&self, key: &K, column: ColumnId) -> Result<bool> {
        Ok(self.inner.contains_key(&column_key(key, column)))
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
