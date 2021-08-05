use crate::state::in_memory::column_key;
use crate::state::{BatchOperations, ColumnId, KeyValueStore, Result, TransactableStorage};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

#[derive(Default, Debug)]
pub struct MemoryStore {
    inner: Arc<Mutex<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::<Vec<u8>, Vec<u8>>::new())),
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
}

impl BatchOperations for MemoryStore {}

impl TransactableStorage for MemoryStore {}
