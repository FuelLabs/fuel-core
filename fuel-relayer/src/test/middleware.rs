use async_trait::async_trait;
use bytes::Bytes;
use ethers_core::types::{
    Block, BlockId, Bytes as EthersBytes, Filter, Log, TxHash, H160, H256, U256, U64,
};
use ethers_providers::{FromErr, Middleware, SyncingStatus};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fmt::Debug, str::FromStr};
use thiserror::Error;
use tokio::sync::Mutex;

type TriggerHandler = Box<dyn Fn(&mut MockData, TriggerType) -> bool + Send>;

pub struct MockMiddleware<M> {
    pub inner: M,
    pub data: Mutex<MockData>,
    pub triggers: Mutex<Vec<TriggerHandler>>,
    pub triggers_index: AtomicUsize,
}

impl<M> fmt::Debug for MockMiddleware<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockMiddleware")
            .field("data", &self.data)
            .finish()
    }
}

#[derive(Debug)]
pub struct MockData {
    pub is_syncing: SyncingStatus,
    pub best_block: Block<TxHash>,
    pub logs_batch: Vec<Vec<Log>>,
    pub logs_batch_index: usize,
}

impl Default for MockData {
    fn default() -> Self {
        let mut best_block = Block::default();
        best_block.hash = Some(H256::from_str("0x0000000000000001").unwrap());
        best_block.number = Some(U64::from(20i32));
        MockData {
            best_block,
            is_syncing: SyncingStatus::IsFalse,
            logs_batch: Vec::new(),
            logs_batch_index: 0,
        }
    }
}

impl<M: Middleware> MockMiddleware<M> {
    /// Instantiates the nonce manager with a 0 nonce. The `address` should be the
    /// address which you'll be sending transactions from
    pub fn new(inner: M, ) -> Self {
        Self {
            inner,
            data: Mutex::new(MockData::default()),
            triggers: Mutex::new(Vec::new()),
            triggers_index: AtomicUsize::new(0),
        }
    }

    pub fn insert_trigger(&mut self) {}

    pub async fn insert_log_batch(&mut self, logs: Vec<Log>) {
        self.data.lock().await.logs_batch.push(logs)
    }

    async fn trigger(&self, trigger_type: TriggerType) {
        if let Some(trigger) = self
            .triggers
            .lock()
            .await
            .get(self.triggers_index.load(Ordering::SeqCst))
        {
            let mut mock_data = self.data.lock().await;
            if trigger(&mut mock_data, trigger_type) {
                self.triggers_index.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            assert!(true, "Badly structured test in Relayer MockMiddleware");
        }
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

pub enum TriggerType {
    Syncing,
    GetBlockNumber,
    GetLogs(Filter),
    GetBlock(BlockId),
    GetFilterChanges(U256),
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
        self.trigger(TriggerType::Syncing);
        Ok(self.data.lock().await.is_syncing.clone())
    }

    /// Used in initial sync to get current best eth block
    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        self.trigger(TriggerType::GetBlockNumber);
        Ok(self.data.lock().await.best_block.number.unwrap())
    }

    /// used for initial sync to get logs of already finalized diffs
    async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>, Self::Error> {
        self.trigger(TriggerType::GetLogs(filter.clone()));
        Ok(Vec::new())
    }

    /// used for initial sync to get block hash. Other fields can be ignored.
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let block_id = block_hash_or_number.into();
        self.trigger(TriggerType::GetBlock(block_id.clone()));
        // TODO change
        Ok(Some(self.data.lock().await.best_block.clone()))
    }

    /// only thing used FilterWatcher
    async fn get_filter_changes<T, R>(&self, id: T) -> Result<Vec<R>, Self::Error>
    where
        T: Into<U256> + Send + Sync,
        R: Serialize + DeserializeOwned + Send + Sync + Debug,
    {
        let block_id = id.into();
        self.trigger(TriggerType::GetFilterChanges(block_id.clone()));

        let data = self.data.lock().await;
        Ok(
            if let Some(logs) = data.logs_batch.get(data.logs_batch_index) {
                let mut ret_logs = Vec::new();
                for log in logs {
                    let log = serde_json::to_value(&log).map_err(|_e| Self::Error::Internal)?;
                    let res: R = serde_json::from_value(log).map_err(|_e| Self::Error::Internal)?;
                    ret_logs.push(res);
                }
                ret_logs
            } else {
                Vec::new()
            },
        )
    }
}

fn log_default() -> Log {
    Log {
        address: H160::zero(),
        topics: Vec::new(),
        data: EthersBytes(Bytes::new()),
        block_hash: None,
        block_number: None,
        transaction_hash: None,
        transaction_index: None,
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
    }
}
