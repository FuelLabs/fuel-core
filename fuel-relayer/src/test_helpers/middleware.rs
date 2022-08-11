use async_trait::async_trait;
use ethers_core::types::{Block, BlockId, Filter, Log, TxHash, H256, U256, U64};
use ethers_providers::{
    FilterWatcher, JsonRpcClient, Middleware, Provider, ProviderError, SyncingStatus,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::io::BufWriter;
use std::sync::Arc;
use std::time::Duration;
use std::{fmt::Debug, str::FromStr};
use thiserror::Error;
use tokio::sync::Mutex;

#[async_trait]
pub trait TriggerHandle: Send {
    async fn run<'a>(&mut self, _data: &mut MockData, _trigger: TriggerType<'a>) {}
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
    pub blocks_batch: Vec<Vec<H256>>,
    pub blocks_batch_index: usize,
}

impl Default for MockData {
    fn default() -> Self {
        let best_block = Block {
            hash: Some(
                H256::from_str(
                    "0xa1ea3121940930f7e7b54506d80717f14c5163807951624c36354202a8bffda6",
                )
                .unwrap(),
            ),
            number: Some(U64::from(20i32)),
            ..Default::default()
        };
        MockData {
            best_block,
            is_syncing: SyncingStatus::IsFalse,
            logs_batch: Vec::new(),
            logs_batch_index: 0,
            blocks_batch: Vec::new(),
            blocks_batch_index: 0,
        }
    }
}

impl Default for MockMiddleware {
    fn default() -> Self {
        // Instantiates the nonce manager with a 0 nonce. The `address` should be the
        // address which you'll be sending transactions from
        let mut s = Self {
            inner: Box::new(None),
            data: Arc::new(Mutex::new(MockData::default())),
            handler: Arc::new(Mutex::new(Box::new(EmptyTriggerHand {}))),
        };
        let sc = s.clone();
        s.inner = Box::new(Some(Provider::new(sc)));
        s
    }
}

impl MockMiddleware {
    pub async fn trigger_handle(&self, trigger_handle: Box<dyn TriggerHandle>) {
        *self.handler.lock().await = trigger_handle;
    }

    pub async fn insert_log_batch(&mut self, logs: Vec<Log>) {
        self.data.lock().await.logs_batch.push(logs)
    }

    async fn trigger<'a>(&self, trigger: TriggerType<'a>) {
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

#[derive(Debug, Eq, PartialEq)]
pub enum TriggerType<'a> {
    Syncing,
    GetBlockNumber,
    GetLogs(&'a Filter),
    GetBlock(BlockId),
    GetLogFilterChanges,
    GetBlockFilterChanges,
}

#[async_trait]
impl JsonRpcClient for MockMiddleware {
    /// A JSON-RPC Error
    type Error = ProviderError;

    /// Sends a request with the provided JSON-RPC and parameters serialized as JSON
    async fn request<T, R>(&self, method: &str, params: T) -> Result<R, Self::Error>
    where
        T: Debug + Serialize + Send + Sync,
        R: DeserializeOwned,
    {
        if method == "eth_getFilterChanges" {
            let buffer = BufWriter::new(Vec::new());
            let mut ser = serde_json::Serializer::new(buffer);
            params.serialize(&mut ser)?;
            let out = ser.into_inner().buffer().to_vec();
            let parameters: Vec<U256> = serde_json::from_slice(&out)?;
            if parameters[0] == U256::zero() {
                self.trigger(TriggerType::GetLogFilterChanges).await;
                // It is logs
                let data = self.data.lock().await;
                let log = data
                    .logs_batch
                    .get(data.logs_batch_index)
                    .cloned()
                    .unwrap_or_default();
                let res = serde_json::to_value(&log)?;
                let res: R = serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            } else {
                self.trigger(TriggerType::GetBlockFilterChanges).await;
                // It is block hashes
                let data = self.data.lock().await;
                let block_hashes = data
                    .blocks_batch
                    .get(data.blocks_batch_index)
                    .cloned()
                    .unwrap_or_default();

                let res = serde_json::to_value(&block_hashes)?;
                let res: R = serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            }
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
        self.trigger(TriggerType::GetLogs(filter)).await;
        Ok(Vec::new())
    }

    /// used for initial sync to get block hash. Other fields can be ignored.
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let block_id = block_hash_or_number.into();
        self.trigger(TriggerType::GetBlock(block_id)).await;
        // TODO change
        Ok(Some(self.data.lock().await.best_block.clone()))
    }

    /// watch blocks
    async fn watch_blocks(&self) -> Result<FilterWatcher<'_, Self::Provider, H256>, Self::Error> {
        let id = U256::one();
        let filter = FilterWatcher::new(id, self.inner.as_ref().as_ref().unwrap())
            .interval(Duration::from_secs(1));
        Ok(filter)
    }

    /// watch logs
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
