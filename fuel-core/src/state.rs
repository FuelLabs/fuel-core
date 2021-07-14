use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, Default)]
pub struct MultiKey<K1: AsRef<[u8]>, K2: AsRef<[u8]>>(pub(crate) (K1, K2));

impl<K1: AsRef<[u8]>, K2: AsRef<[u8]>> AsRef<[u8]> for MultiKey<K1, K2> {
    fn as_ref(&self) -> &[u8] {
        todo!()
    }
}

pub trait KeyValueStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
{
    fn get(&self, key: &K) -> Result<Option<V>>;
    fn put(&mut self, key: K, value: V) -> Result<Option<V>>;
    fn delete(&mut self, key: &K) -> Result<Option<V>>;
    fn exists(&self, key: &K) -> Result<bool>;
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("error performing binary serialization")]
    Codec,
    #[error("error occurred in the underlying datastore `{0}`")]
    DatabaseError(Box<dyn std::error::Error>),
}

pub trait BatchOperations<K, V>: KeyValueStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
{
    fn batch_write<I>(&mut self, entries: I) -> Result<()>
    where
        I: Iterator<Item = WriteOperation<K, V>>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WriteOperation<K, V> {
    Insert(K, V),
    Remove(K),
}

pub trait Transaction<K, V, View>: KeyValueStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
    View: KeyValueStore<K, V>,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut View) -> TransactionResult<R> + Copy;
}

pub type TransactionResult<T> = core::result::Result<T, TransactionError>;

pub trait TransactionalStorage<K, V>: KeyValueStore<K, V> + BatchOperations<K, V> + Sized
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
