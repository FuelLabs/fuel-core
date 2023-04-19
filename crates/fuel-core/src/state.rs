use crate::database::{
    Column,
    Result as DatabaseResult,
};
use fuel_core_storage::iter::BoxedIter;
use std::{
    fmt::Debug,
    sync::Arc,
};

pub type DataSource = Arc<dyn TransactableStorage>;
pub type Value = Arc<Vec<u8>>;
pub type KVItem = DatabaseResult<(Vec<u8>, Value)>;

pub trait KeyValueStore {
    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;
    fn put(
        &self,
        key: &[u8],
        column: Column,
        value: Value,
    ) -> DatabaseResult<Option<Value>>;
    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;
    fn exists(&self, key: &[u8], column: Column) -> DatabaseResult<bool>;
    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem>;
}

#[derive(Copy, Clone, Debug, PartialOrd, Eq, PartialEq)]
pub enum IterDirection {
    Forward,
    Reverse,
}

impl Default for IterDirection {
    fn default() -> Self {
        Self::Forward
    }
}

pub trait BatchOperations: KeyValueStore {
    fn batch_write(
        &self,
        entries: &mut dyn Iterator<Item = (Vec<u8>, Column, WriteOperation)>,
    ) -> DatabaseResult<()> {
        for (key, column, op) in entries {
            match op {
                // TODO: error handling
                WriteOperation::Insert(value) => {
                    let _ = self.put(&key, column, value);
                }
                WriteOperation::Remove => {
                    let _ = self.delete(&key, column);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum WriteOperation {
    Insert(Value),
    Remove,
}

pub trait TransactableStorage: BatchOperations + Debug + Send + Sync {}

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
