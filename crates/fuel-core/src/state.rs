use crate::database::{
    Column,
    Result as DatabaseResult,
};
use fuel_core_storage::iter::{
    BoxedIter,
    IterDirection,
};
use std::{
    fmt::Debug,
    sync::Arc,
};

pub type DataSource = Arc<dyn TransactableStorage>;
pub type Value = Arc<Vec<u8>>;
pub type KVItem = DatabaseResult<(Vec<u8>, Value)>;

pub trait KeyValueStore {
    fn put(
        &self,
        key: &[u8],
        column: Column,
        value: Value,
    ) -> DatabaseResult<Option<Value>>;

    fn write(&self, key: &[u8], column: Column, buf: &[u8]) -> DatabaseResult<usize>;

    fn replace(
        &self,
        key: &[u8],
        column: Column,
        buf: &[u8],
    ) -> DatabaseResult<(usize, Option<Value>)>;

    fn take(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;

    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;

    fn exists(&self, key: &[u8], column: Column) -> DatabaseResult<bool>;

    fn size_of_value(&self, key: &[u8], column: Column) -> DatabaseResult<Option<usize>>;

    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;

    fn read(
        &self,
        key: &[u8],
        column: Column,
        buf: &mut [u8],
    ) -> DatabaseResult<Option<usize>>;

    fn read_alloc(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Value>>;

    fn iter_all(
        &self,
        column: Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem>;
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
