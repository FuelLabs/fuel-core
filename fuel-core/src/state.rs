use crate::state::in_memory::transaction::MemoryTransactionView;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;
pub type DataSource<K, V> = Arc<Mutex<dyn TransactableStorage<K, V>>>;
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

pub trait KeyValueStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
{
    fn get(&self, key: &K, column: ColumnId) -> Result<Option<V>>;
    fn put(&mut self, key: K, column: ColumnId, value: V) -> Result<Option<V>>;
    fn delete(&mut self, key: &K, column: ColumnId) -> Result<Option<V>>;
    fn exists(&self, key: &K, column: ColumnId) -> Result<bool>;
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("error performing binary serialization")]
    Codec,
    #[error("error occurred in the underlying datastore `{0}`")]
    DatabaseError(Box<dyn std::error::Error + Send>),
}

pub trait BatchOperations<K, V>: KeyValueStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
{
    fn batch_write(
        &mut self,
        entries: &mut dyn Iterator<Item = WriteOperation<K, V>>,
    ) -> Result<()> {
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
pub enum WriteOperation<K, V> {
    Insert(K, ColumnId, V),
    Remove(K, ColumnId),
}

pub trait Transaction<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionView<K, V>) -> TransactionResult<R> + Copy;
}

pub type TransactionResult<T> = core::result::Result<T, TransactionError>;

pub trait TransactableStorage<K, V>:
    KeyValueStore<K, V> + BatchOperations<K, V> + Debug + Send
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Serialize + DeserializeOwned + Debug + Clone,
{
}

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
#[cfg(feature = "sled-db")]
pub mod sled_db;

pub mod store {}
