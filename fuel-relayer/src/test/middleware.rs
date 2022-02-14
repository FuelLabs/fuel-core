use async_trait::async_trait;
use ethers_core::types::{Block, BlockId, Filter, Log, TxHash, U256, U64};
use ethers_providers::{FromErr, Middleware, SyncingStatus};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug)]
pub struct MockMiddleware<M> {
    inner: M,
}

impl<M: Middleware> MockMiddleware<M> {
    /// Instantiates the nonce manager with a 0 nonce. The `address` should be the
    /// address which you'll be sending transactions from
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

#[derive(Error, Debug)]
/// Thrown when an error happens at the Nonce Manager
pub enum MockMiddlewareError<M: Middleware> {
    /// Thrown when the internal middleware errors
    #[error("{0}")]
    MiddlewareError(M::Error),
    #[error("Internal error")]
    Internal,
}

impl<M: Middleware> FromErr<M::Error> for MockMiddlewareError<M> {
    fn from(src: M::Error) -> Self {
        Self::MiddlewareError(src)
    }
}

/*
WHAT DO I NEED FOR RELAYER FROM PROVIDER:
* syncing API
* get_block_number API
* get_logs API.
* .watch() API for logs with filter. Impl LogStream
    * LogsWatcher only uses .next()
* get_block API using only HASH
*/

#[async_trait]
impl<M> Middleware for MockMiddleware<M>
where
    M: Middleware,
{
    type Error = MockMiddlewareError<M>;
    type Provider = M::Provider;
    type Inner = M;

    fn inner(&self) -> &M {
        &self.inner
    }

    /// Needs for initial sync of relayer
    async fn syncing(&self) -> Result<SyncingStatus, Self::Error> {
        Ok(SyncingStatus::IsFalse)
    }

    /// Used in initial sync to get current best eth block
    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        Ok(U64::zero())
    }

    /// used for initial sync to get logs of already finalized diffs
    async fn get_logs(&self, _filter: &Filter) -> Result<Vec<Log>, Self::Error> {
        Ok(Vec::new())
    }

    /// used for initial sync to get block hash. Other fields can be ignored.
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        _block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        Ok(None)
    }

    /// only thing used FilterWatcher
    async fn get_filter_changes<T, R>(&self, _id: T) -> Result<Vec<R>, Self::Error>
    where
        T: Into<U256> + Send + Sync,
        R: Serialize + DeserializeOwned + Send + Sync + Debug,
    {
        Err(MockMiddlewareError::Internal)
        //self.inner().get_filter_changes(id).await.map_err(FromErr::from)
    }
}
