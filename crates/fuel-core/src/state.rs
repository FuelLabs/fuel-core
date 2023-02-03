use crate::{
    database::{
        Column,
        Result as DatabaseResult,
    },
    state::in_memory::transaction::MemoryTransactionView,
};
use fuel_core_storage::iter::BoxedIter;
use std::{
    fmt::Debug,
    sync::Arc,
};

pub type DataSource = Arc<dyn TransactableStorage>;
pub type ColumnId = u32;
pub type KVItem = DatabaseResult<(Vec<u8>, Vec<u8>)>;

pub trait KeyValueStore {
    fn get(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>>;
    fn put(
        &self,
        key: &[u8],
        column: Column,
        value: Vec<u8>,
    ) -> DatabaseResult<Option<Vec<u8>>>;
    fn delete(&self, key: &[u8], column: Column) -> DatabaseResult<Option<Vec<u8>>>;
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
        entries: &mut dyn Iterator<Item = WriteOperation>,
    ) -> DatabaseResult<()> {
        for entry in entries {
            match entry {
                // TODO: error handling
                WriteOperation::Insert(key, column, value) => {
                    let _ = self.put(&key, column, value);
                }
                WriteOperation::Remove(key, column) => {
                    let _ = self.delete(&key, column);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum WriteOperation {
    Insert(Vec<u8>, Column, Vec<u8>),
    Remove(Vec<u8>, Column),
}

pub trait Transaction {
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView) -> TransactionResult<R> + Copy;
}

pub type TransactionResult<T> = core::result::Result<T, TransactionError>;

pub trait TransactableStorage: BatchOperations + Debug + Send + Sync {}

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
