use crate::state::in_memory::{column_key, is_column};
use crate::state::{BatchOperations, ColumnId, KeyValueStore, Result, TransactableStorage};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Mutex;

#[derive(Default, Debug)]
pub struct MemoryStore {
    inner: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::<Vec<u8>, Vec<u8>>::new()),
        }
    }
}

impl KeyValueStore for MemoryStore {
    fn get(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .get(&column_key(key, column))
            .cloned())
    }

    fn put(&self, key: Vec<u8>, column: ColumnId, value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .insert(column_key(&key, column), value))
    }

    fn delete(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .remove(&column_key(key, column)))
    }

    fn exists(&self, key: &[u8], column: ColumnId) -> Result<bool> {
        Ok(self
            .inner
            .lock()
            .expect("poisoned")
            .contains_key(&column_key(key, column)))
    }

    fn iter_all(&self, column: ColumnId) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)>> {
        // clone entire set so we can drop the lock
        let copy: Vec<(Vec<u8>, Vec<u8>)> = self
            .inner
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|(key, _)| is_column(key, column))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Box::new(copy.into_iter())
    }
}

impl BatchOperations for MemoryStore {}

impl TransactableStorage for MemoryStore {}
