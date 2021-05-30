use crate::state::in_memory::transaction_proxy::MemoryTransactionProxy;
use crate::state::{BatchOperations, KeyValueStore, TransactionResult, Transactional, TransactionalProxy};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::future::Future;

pub struct Transaction<K, V, S> {
    proxy: MemoryTransactionProxy<K, V, S>,
}

impl<K, V, S> Transaction<K, V, S>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
    V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
    S: KeyValueStore<Key = K, Value = V> + BatchOperations<Key = K, Value = V>,
{
    pub fn new(store: S) -> Self {
        Self {
            proxy: MemoryTransactionProxy::new(store),
        }
    }
}

#[async_trait::async_trait]
impl<K, V, S> Transactional<K, V, S, MemoryTransactionProxy<K, V, S>> for Transaction<K, V, S>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Send + Sync + Debug + Clone,
    V: Into<Vec<u8>> + Send + Sync + Serialize + DeserializeOwned + Debug + Clone,
    S: KeyValueStore<Key = K, Value = V> + BatchOperations<Key = K, Value = V> + Sync + Send,
{
    async fn transaction<F, Fut, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut MemoryTransactionProxy<K, V, S>) -> Fut + Send,
        Fut: Future<Output = TransactionResult<R>> + Send,
        R: Send,
    {
        let result = f(&mut self.proxy).await;
        self.proxy.commit().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::in_memory::memory_store::MemoryStore;

    #[tokio::test]
    async fn transaction_commit_is_applied_if_successful() {
        let store = MemoryStore::<String, String>::new();
        let mut transaction = Transaction::new(store);

        transaction
            .transaction(|store| async {
                store.put("k1".to_string(), "v1".to_string()).await;
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(store.get("k1".to_string()).await.unwrap(), "v1")
    }

    #[tokio::test]
    async fn transaction_commit_is_not_applied_if_aborted() {
        todo!()
    }
}
