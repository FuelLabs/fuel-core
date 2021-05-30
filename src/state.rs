use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::future::Future;

pub trait Database {}

#[async_trait::async_trait]
pub trait KeyValueStore {
    type Key: AsRef<[u8]> + Debug + Send + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    async fn get(&self, key: Self::Key) -> Option<Self::Value>;
    async fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value>;
    async fn delete(&mut self, key: Self::Key) -> Option<Self::Value>;
    async fn exists(&self, key: Self::Key) -> bool;
}

#[async_trait::async_trait]
pub trait BatchOperations {
    type Key: AsRef<[u8]> + Debug + Send + Clone;
    type Value: Debug + DeserializeOwned + Clone;

    async fn batch_write<I>(&mut self, entries: I)
    where
        I: Iterator<Item = WriteOperation<Self::Key, Self::Value>> + Send;
}

#[derive(Debug)]
enum WriteOperation<K, V> {
    Insert(K, V),
    Remove(K),
}

#[async_trait::async_trait]
pub trait TransactionalProxy {
    type UnderlyingStore: BatchOperations;
    async fn commit(&mut self);
}

#[async_trait::async_trait]
pub trait Transactional<K, V, S, P>
where
    K: AsRef<[u8]> + Debug + Send + Clone,
    V: Debug + DeserializeOwned + Clone,
    S: KeyValueStore<Key = K, Value = V>,
    P: KeyValueStore<Key = K, Value = V>,
{
    async fn transaction<F, Fut, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut P) -> Fut + Send,
        Fut: Future<Output = TransactionResult<R>> + Send,
        R: Send;
}

pub type TransactionResult<T> = Result<T, TransactionError>;

#[derive(Clone, Debug)]
pub enum TransactionError {
    Aborted,
}

pub mod in_memory;
