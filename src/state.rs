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
pub trait Transactional {
    type Store: KeyValueStore + Send;
    async fn transaction<F, Fut, R, E>(&mut self, f: F) -> TransactionResult<R, E>
    where
        F: FnOnce(&mut Self::Store) -> Fut + Send,
        Fut: Future<Output = TransactionResult<R, E>> + Send;
}

pub type TransactionResult<T, E> = Result<T, TransactionError<E>>;

pub enum TransactionError<T> {
    Aborted(T),
}

pub mod in_memory;
