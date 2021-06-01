use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub trait KeyValueStore {
    type Key: AsRef<[u8]> + Debug + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    fn get(&self, key: Self::Key) -> Option<Self::Value>;
    fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value>;
    fn delete(&mut self, key: Self::Key) -> Option<Self::Value>;
    fn exists(&self, key: Self::Key) -> bool;
}

/// Used to indicate if a type may be transacted upon
pub trait Transactional<K, V>: KeyValueStore<Key = K, Value = V> + BatchOperations<Key = K, Value = V> {
    type View: KeyValueStore<Key = K, Value = V>;
}

pub trait BatchOperations {
    type Key: AsRef<[u8]> + Debug + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    fn batch_write<I>(&mut self, entries: I)
    where
        I: Iterator<Item = WriteOperation<Self::Key, Self::Value>>;
}

#[derive(Debug)]
pub enum WriteOperation<K, V> {
    Insert(K, V),
    Remove(K),
}

pub trait Transaction<K, V, View>: KeyValueStore<Key = K, Value = V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + DeserializeOwned + Clone,
    View: KeyValueStore<Key = K, Value = V>,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut View) -> TransactionResult<R>;
}

pub type TransactionResult<T> = Result<T, TransactionError>;

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
