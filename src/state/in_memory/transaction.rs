use crate::state::in_memory::transaction_proxy::MemoryTransactionProxy;
use crate::state::{KeyValueStore, TransactionResult, Transactional};
use std::future::Future;

pub struct Transaction<K, V, S> {
    store: S,
    proxy: MemoryTransactionProxy<K, V, S>,
}

#[async_trait::async_trait]
impl<K, V, S> Transactional for Transaction<K, V, S>
where
    S: KeyValueStore<Key = K, Value = V> + Send,
    K: Send,
    V: Send,
{
    type Store = S;

    async fn transaction<F, Fut, R, E>(&mut self, f: F) -> TransactionResult<R, E>
    where
        F: FnOnce(&mut Self::Store) -> Fut + Send,
        Fut: Future<Output = TransactionResult<R, E>> + Send,
        S: KeyValueStore<Key = K, Value = V> + Send,
    {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transaction_commit_is_applied_if_successful() {
        todo!()
    }

    #[tokio::test]
    async fn transaction_commit_is_not_applied_if_aborted() {
        todo!()
    }
}
