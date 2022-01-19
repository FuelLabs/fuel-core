use crate::state::in_memory::transaction::MemoryTransactionView;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;
pub type DataSource = Arc<dyn TransactableStorage>;
pub type ColumnId = u32;

#[derive(Clone, Debug, Default)]
pub struct MultiKey<K1: AsRef<[u8]>, K2: AsRef<[u8]>> {
    _marker_1: PhantomData<K1>,
    _marker_2: PhantomData<K2>,
    inner: Vec<u8>,
}

impl<K1: AsRef<[u8]>, K2: AsRef<[u8]>> MultiKey<K1, K2> {
    pub fn new(key: (K1, K2)) -> Self {
        Self {
            _marker_1: Default::default(),
            _marker_2: Default::default(),
            inner: key
                .0
                .as_ref()
                .iter()
                .chain(key.1.as_ref().iter())
                .copied()
                .collect(),
        }
    }
}

impl<K1: AsRef<[u8]>, K2: AsRef<[u8]>> AsRef<[u8]> for MultiKey<K1, K2> {
    fn as_ref(&self) -> &[u8] {
        self.inner.as_slice()
    }
}

impl<K1: AsRef<[u8]>, K2: AsRef<[u8]>> From<MultiKey<K1, K2>> for Vec<u8> {
    fn from(key: MultiKey<K1, K2>) -> Vec<u8> {
        key.inner
    }
}

pub trait KeyValueStore {
    fn get(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: Vec<u8>, column: ColumnId, value: Vec<u8>) -> Result<Option<Vec<u8>>>;
    fn delete(&self, key: &[u8], column: ColumnId) -> Result<Option<Vec<u8>>>;
    fn exists(&self, key: &[u8], column: ColumnId) -> Result<bool>;
    fn iter_all(
        &self,
        column: ColumnId,
        prefix: Option<Vec<u8>>,
        start: Option<Vec<u8>>,
        direction: IterDirection,
    ) -> Box<dyn Iterator<Item = (Vec<u8>, Vec<u8>)> + '_>;
}

#[derive(Copy, Clone, Debug, PartialOrd, PartialEq)]
pub enum IterDirection {
    Forward,
    Reverse,
}

impl Default for IterDirection {
    fn default() -> Self {
        Self::Forward
    }
}

pub use interfaces::db::Error;

pub trait BatchOperations: KeyValueStore {
    fn batch_write(&self, entries: &mut dyn Iterator<Item = WriteOperation>) -> Result<()> {
        for entry in entries {
            match entry {
                // TODO: error handling
                WriteOperation::Insert(key, column, value) => {
                    let _ = self.put(key, column, value);
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
    Insert(Vec<u8>, ColumnId, Vec<u8>),
    Remove(Vec<u8>, ColumnId),
}

pub trait Transaction {
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView) -> TransactionResult<R> + Copy;
}

pub type TransactionResult<T> = core::result::Result<T, TransactionError>;

pub trait TransactableStorage: KeyValueStore + BatchOperations + Debug + Send + Sync {}

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
#[cfg(feature = "rocksdb")]
pub mod rocks_db;
