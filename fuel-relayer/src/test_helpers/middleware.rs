use async_trait::async_trait;
use ethers_core::types::{Block, BlockId, Filter, Log, TxHash, H256, U256, U64};
use ethers_providers::{
    FilterWatcher, JsonRpcClient, Middleware, Provider, ProviderError, SyncingStatus,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use std::{fmt::Debug, str::FromStr};
use thiserror::Error;
use tokio::sync::Mutex;

#[async_trait]
pub trait TriggerHandle: Send {
    async fn run(&mut self, _data: &mut MockData, _trigger: TriggerType) {}
}

pub struct EmptyTriggerHand {}
impl TriggerHandle for EmptyTriggerHand {}

#[derive(Clone)]
pub struct MockMiddleware {
    pub inner: Box<Option<Provider<MockMiddleware>>>,
    pub data: Arc<Mutex<MockData>>,
    pub handler: Arc<Mutex<Box<dyn TriggerHandle>>>,
}

impl fmt::Debug for MockMiddleware {
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
        best_block.hash = Some(
            H256::from_str("0xa1ea3121940930f7e7b54506d80717f14c5163807951624c36354202a8bffda6")
                .unwrap(),
        );
        best_block.number = Some(U64::from(20i32));
        MockData {
            best_block,
            is_syncing: SyncingStatus::IsFalse,
            logs_batch: Vec::new(),
            logs_batch_index: 0,
        }
    }
}

impl MockMiddleware {
    pub async fn trigger_handle(&self, trigger_handle: Box<dyn TriggerHandle>) {
        *self.handler.lock().await = trigger_handle;
    }
    /// Instantiates the nonce manager with a 0 nonce. The `address` should be the
    /// address which you'll be sending transactions from
    pub fn new() -> Self {
        let mut s = Self {
            inner: Box::new(None),
            data: Arc::new(Mutex::new(MockData::default())),
            handler: Arc::new(Mutex::new(Box::new(EmptyTriggerHand {}))),
        };
        let sc = s.clone();
        s.inner = Box::new(Some(Provider::new(sc)));
        s
    }

    pub async fn insert_log_batch(&mut self, logs: Vec<Log>) {
        self.data.lock().await.logs_batch.push(logs)
    }

    async fn trigger(&self, trigger: TriggerType) {
        let mut data = self.data.lock().await;
        let mut handler = self.handler.lock().await;
        handler.run(&mut data, trigger).await
    }
}

#[derive(Error, Debug)]
/// Thrown when an error happens at the Nonce Manager
pub enum MockMiddlewareError {
    /// Thrown when the internal middleware errors
    #[error("Test")]
    MiddlewareError(),
    #[error("Internal error")]
    Internal,
}

#[derive(Debug, PartialEq)]
pub enum TriggerType {
    Syncing,
    GetBlockNumber,
    GetLogs(Filter),
    GetBlock(BlockId),
    GetFilterChanges(U256),
}

#[async_trait]
impl JsonRpcClient for MockMiddleware {
    /// A JSON-RPC Error
    type Error = ProviderError;

    /// Sends a request with the provided JSON-RPC and parameters serialized as JSON
    async fn request<T, R>(&self, method: &str, _params: T) -> Result<R, Self::Error>
    where
        T: Debug + Serialize + Send + Sync,
        R: DeserializeOwned,
    {
        if method == "eth_getFilterChanges" {
            let block_id = U256::zero();
            self.trigger(TriggerType::GetFilterChanges(block_id.clone()))
                .await;

            let data = self.data.lock().await;
            return Ok(
                if let Some(logs) = data.logs_batch.get(data.logs_batch_index) {
                    let mut ret_logs = Vec::new();
                    for log in logs {
                        // let log = serde_json::to_value(&log).map_err(|e| Self::Error::SerdeJson(e))?;
                        // let res: R =
                        //    ;
                        ret_logs.push(log);
                    }
                    //ret_logs
                    let res =
                        serde_json::to_value(&ret_logs).map_err(|e| Self::Error::SerdeJson(e))?;
                    let res: R =
                        serde_json::from_value(res).map_err(|e| Self::Error::SerdeJson(e))?;
                    res
                } else {
                    let ret: Vec<Log> = Vec::new();
                    let res = serde_json::to_value(ret)?;
                    let res: R =
                        serde_json::from_value(res).map_err(|e| Self::Error::SerdeJson(e))?;
                    res
                },
            );
        } else {
            panic!("Request not mocked: {}", method);
        }
    }
}

/*
Needed functionality for relayer to function:
* syncing API
* get_block_number API
* get_logs API.
* .watch() API for logs with filter. Impl LogStream
    * LogsWatcher only uses .next()
* get_block API using only HASH
*/

#[async_trait]
impl Middleware for MockMiddleware {
    type Error = ProviderError;
    type Provider = Self;
    type Inner = Self;

    fn inner(&self) -> &Self::Inner {
        unreachable!("There is no inner provider here")
    }

    /// Needs for initial sync of relayer
    async fn syncing(&self) -> Result<SyncingStatus, Self::Error> {
        self.trigger(TriggerType::Syncing).await;
        Ok(self.data.lock().await.is_syncing.clone())
    }

    /// Used in initial sync to get current best eth block
    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        let this = self;
        let _ = this.trigger(TriggerType::GetBlockNumber).await;
        Ok(self.data.lock().await.best_block.number.unwrap())
    }

    /// used for initial sync to get logs of already finalized diffs
    async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>, Self::Error> {
        self.trigger(TriggerType::GetLogs(filter.clone())).await;
        Ok(Vec::new())
    }

    /// used for initial sync to get block hash. Other fields can be ignored.
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let block_id = block_hash_or_number.into();
        self.trigger(TriggerType::GetBlock(block_id.clone())).await;
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
        self.trigger(TriggerType::GetFilterChanges(block_id.clone()))
            .await;

        let data = self.data.lock().await;
        Ok(
            if let Some(logs) = data.logs_batch.get(data.logs_batch_index) {
                let mut ret_logs = Vec::new();
                for log in logs {
                    let log = serde_json::to_value(&log).map_err(|e| Self::Error::SerdeJson(e))?;
                    let res: R =
                        serde_json::from_value(log).map_err(|e| Self::Error::SerdeJson(e))?;
                    ret_logs.push(res);
                }
                ret_logs
            } else {
                Vec::new()
            },
        )
    }

    async fn watch<'b>(
        &'b self,
        _filter: &Filter,
    ) -> Result<FilterWatcher<'b, Self::Provider, Log>, Self::Error> {
        let id = U256::zero();
        let filter = FilterWatcher::new(id, self.inner.as_ref().as_ref().unwrap())
            .interval(Duration::from_secs(1));
        Ok(filter)
    }
}
