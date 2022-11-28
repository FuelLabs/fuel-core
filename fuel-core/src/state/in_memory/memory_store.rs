use crate::{
    database::Column,
    state::{
        BatchOperations,
        IterDirection,
        KeyValueStore,
        Result,
        TransactableStorage,
    },
};
use itertools::Itertools;
use std::{
    collections::HashMap,
    fmt::Debug,
    sync::Mutex,
};
use strum::EnumCount;

#[derive(Default, Debug)]
pub struct MemoryStore {
    inner: [Mutex<HashMap<Vec<u8>, Vec<u8>>>; Column::COUNT],
}

impl KeyValueStore for MemoryStore {
    fn get(&self, key: &[u8], column: Column) -> Result<Option<Vec<u8>>> {
        Ok(self.inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned")
            .get(key)
            .cloned())
    }

    fn read(&self, key: &[u8], column: Column, buf: &mut [u8]) -> Result<Option<usize>> {
        let lock = self.inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned");

        match lock.get(key) {
            Some(p) => {
                let p: &[u8] = p.as_ref();
                if p.len() > buf.len() {
                    Err(crate::state::Error::ReadOverflow)
                } else {
                    // Safe index due to above check.
                    buf[..p.len()].copy_from_slice(p);
                    Ok(Some(p.len()))
                }
            }
            None => Ok(None),
        }
    }

    fn put(&self, key: &[u8], column: Column, value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        Ok(self.inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned")
            .insert(key.to_vec(), value))
    }

    fn delete(&self, key: &[u8], column: Column) -> Result<Option<Vec<u8>>> {
        Ok(self.inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned")
            .remove(key))
    }

    fn exists(&self, key: &[u8], column: Column) -> Result<bool> {
        Ok(self.inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned")
            .contains_key(key))
    }

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + '_> {
        // clone entire set so we can drop the lock
        let mut copy: Vec<(Vec<u8>, Vec<u8>)> = self
            .inner[Column::to_index(&column)]
            .lock()
            .expect("poisoned")
            .iter()
            // strip column
            .map(|(key, value)| (key.clone(), value.clone()))
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
            Box::new(
                copy.into_iter()
                    .skip_while(move |(key, _)| key.as_slice() != start.as_slice())
                    .map(Ok),
            )
        } else {
            Box::new(copy.into_iter().map(Ok))
        }
    }
}

impl BatchOperations for MemoryStore {}

impl TransactableStorage for MemoryStore {}
