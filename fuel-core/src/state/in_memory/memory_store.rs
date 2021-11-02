use crate::state::{
    in_memory::{column_key, is_column},
    BatchOperations, ColumnId, IterDirection, KeyValueStore, Result, TransactableStorage,
};
use itertools::Itertools;
use std::{collections::HashMap, fmt::Debug, mem::size_of, sync::Mutex};

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

    fn iter_all(
        &self,
        column: ColumnId,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_> {
        // clone entire set so we can drop the lock
        let mut copy: Vec<(Vec<u8>, Vec<u8>)> = self
            .inner
            .lock()
            .expect("poisoned")
            .iter()
            .filter(|(key, _)| is_column(key, column))
            // strip column
            .map(|(key, value)| (key[size_of::<ColumnId>()..].to_vec(), value.clone()))
            // filter prefix
            .filter(|(key, _)| {
                if let Some(prefix) = &prefix {
                    key.starts_with(prefix.as_slice())
                } else {
                    true
                }
            })
            .sorted()
            .collect();

        if direction == IterDirection::Reverse {
            copy.reverse();
        }

        if let Some(start) = start {
            let start = start.to_vec();
            Box::new(
                copy.into_iter()
                    .skip_while(move |(key, _)| key.as_slice() != start.as_slice()),
            )
        } else {
            Box::new(copy.into_iter())
        }
    }
}

impl BatchOperations for MemoryStore {}

impl TransactableStorage for MemoryStore {}
