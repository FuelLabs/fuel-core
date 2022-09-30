use async_trait::async_trait;
use ethers_core::types::{
    Block,
    BlockId,
    Filter,
    Log,
    Transaction,
    TransactionReceipt,
    TxHash,
    H256,
    U64,
};
use ethers_providers::{
    JsonRpcClient,
    LogQuery,
    Middleware,
    Provider,
    ProviderError,
    SyncingStatus,
};
use fuel_core_interfaces::common::prelude::Bytes32;
use parking_lot::Mutex;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    collections::HashMap,
    fmt,
    fmt::Debug,
    str::FromStr,
    sync::Arc,
};
use thiserror::Error;

#[derive(Clone)]
pub struct MockMiddleware {
    pub inner: Box<Option<Provider<MockMiddleware>>>,
    data: Arc<parking_lot::Mutex<InnerState>>,
    before_event: Arc<Mutex<Option<EventFn>>>,
    after_event: Arc<Mutex<Option<EventFn>>>,
}

pub type EventFn = Box<dyn for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync>;

#[derive(Default)]
struct InnerState {
    data: MockData,
    override_fn: Option<Box<dyn FnMut(&mut MockData) + Send + Sync>>,
}

#[derive(Debug)]
pub struct MockData {
    pub is_syncing: SyncingStatus,
    pub best_block: Block<TxHash>,
    pub logs_batch: Vec<Vec<Log>>,
    pub logs_batch_index: usize,
    pub blocks_batch: Vec<Vec<H256>>,
    pub blocks_batch_index: usize,
    // mapping(bytes32 => uint256) public s_incomingMessageRoots;
    pub incoming_message_roots: HashMap<Bytes32, H256>,
}

impl MockMiddleware {
    fn before_event<'a>(&self, trigger: TriggerType<'a>) {
        let mut be = self.before_event.lock();
        if let Some(be) = be.as_mut() {
            self.update_data(|data| be(data, trigger))
        }
    }

    fn after_event<'a>(&self, trigger: TriggerType<'a>) {
        let mut ae = self.after_event.lock();
        if let Some(ae) = ae.as_mut() {
            self.update_data(|data| ae(data, trigger))
        }
    }

    pub fn update_data<R>(&self, delta: impl FnOnce(&mut MockData) -> R) -> R {
        self.data.lock().update(delta)
    }

    pub fn set_before_event(
        &self,
        f: impl for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync + 'static,
    ) {
        *self.before_event.lock() = Some(Box::new(f));
    }

    pub fn set_after_event(
        &self,
        f: impl for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync + 'static,
    ) {
        *self.after_event.lock() = Some(Box::new(f));
    }

    pub fn set_state_override(
        &self,
        f: impl FnMut(&mut MockData) + Send + Sync + 'static,
    ) {
        self.data.lock().override_fn = Some(Box::new(f));
    }
}

impl InnerState {
    fn update<R>(&mut self, delta: impl FnOnce(&mut MockData) -> R) -> R {
        let r = delta(&mut self.data);
        let f = self.override_fn.as_mut();
        if let Some(f) = f {
            f(&mut self.data);
        }
        r
    }
}

impl Debug for InnerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InnerState")
            .field("data", &self.data)
            .finish()
    }
}

impl fmt::Debug for MockMiddleware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockMiddleware")
            .field("data", &self.data)
            .finish()
    }
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
            incoming_message_roots: Default::default(),
        }
    }
}

impl Default for MockMiddleware {
    fn default() -> Self {
        // Instantiates the nonce manager with a 0 nonce. The `address` should be the
        // address which you'll be sending transactions from
        let mut s = Self {
            inner: Box::new(None),
            data: Arc::new(Mutex::new(InnerState::default())),
            before_event: Arc::new(Mutex::new(None)),
            after_event: Arc::new(Mutex::new(None)),
        };
        let sc = s.clone();
        s.inner = Box::new(Some(Provider::new(sc)));
        s
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
    Send,
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
        match method {
            "eth_getTransactionByHash" => {
                let mut txn = Transaction::default();
                txn.block_number = self.update_data(|data| data.best_block.number);
                let res = serde_json::to_value(Some(txn))?;
                let res: R =
                    serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            }
            "eth_getTransactionReceipt" => {
                let mut txn = TransactionReceipt::default();
                txn.block_number = self.update_data(|data| {
                    data.best_block.number = Some(data.best_block.number.unwrap() + 1u64);
                    data.best_block.number
                });
                let res = serde_json::to_value(Some(txn))?;
                let res: R =
                    serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            }
            "eth_blockNumber" => {
                let r = self.get_block_number().await.unwrap();
                let res = serde_json::to_value(r)?;
                let res: R =
                    serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            }
            "eth_getLogs" => {
                let params = serde_json::to_value(params)?;
                let params: Vec<Filter> =
                    serde_json::from_value(params).map_err(Self::Error::SerdeJson)?;
                let r = self.get_logs(&params[0]).await.unwrap();
                let res = serde_json::to_value(r)?;
                let res: R =
                    serde_json::from_value(res).map_err(Self::Error::SerdeJson)?;
                Ok(res)
            }
            _ => panic!("Request not mocked: {}", method),
        }
    }
}

// Needed functionality for relayer to function:
// syncing API
// get_block_number API
// get_logs API.
// .watch() API for logs with filter. Impl LogStream
// LogsWatcher only uses .next()
// get_block API using only HASH

#[async_trait]
impl Middleware for MockMiddleware {
    type Error = ProviderError;
    type Provider = Self;
    type Inner = Self;

    fn inner(&self) -> &Self::Inner {
        unreachable!("There is no inner provider here")
    }

    fn provider(&self) -> &Provider<Self::Provider> {
        self.inner.as_ref().as_ref().unwrap()
    }

    /// Needs for initial sync of relayer
    async fn syncing(&self) -> Result<SyncingStatus, Self::Error> {
        self.before_event(TriggerType::Syncing);
        let r = Ok(self.update_data(|data| data.is_syncing.clone()));
        self.after_event(TriggerType::Syncing);
        r
    }

    /// Used in initial sync to get current best eth block
    async fn get_block_number(&self) -> Result<U64, Self::Error> {
        let this = self;
        let _ = this.before_event(TriggerType::GetBlockNumber);
        let r = Ok(self.update_data(|data| data.best_block.number.unwrap()));
        self.after_event(TriggerType::GetBlockNumber);
        r
    }

    /// used for initial sync to get logs of already finalized diffs
    async fn get_logs(&self, filter: &Filter) -> Result<Vec<Log>, Self::Error> {
        self.before_event(TriggerType::GetLogs(filter));
        let r = self.update_data(|data| {
            data.logs_batch
                .iter()
                .flat_map(|logs| {
                    logs.iter().filter_map(|log| {
                        let r = match filter.address.as_ref()? {
                            ethers_core::types::ValueOrArray::Value(v) => {
                                log.address == *v
                            }
                            ethers_core::types::ValueOrArray::Array(v) => {
                                v.iter().any(|v| log.address == *v)
                            }
                        };
                        let log_block_num = log.block_number?;
                        let r = r
                            && log_block_num
                                >= filter.block_option.get_from_block()?.as_number()?
                            && log_block_num
                                <= filter.block_option.get_to_block()?.as_number()?;
                        r.then(|| log)
                    })
                })
                .cloned()
                .collect()
        });
        self.after_event(TriggerType::GetLogs(filter));
        Ok(r)
    }

    /// used for initial sync to get block hash. Other fields can be ignored.
    async fn get_block<T: Into<BlockId> + Send + Sync>(
        &self,
        block_hash_or_number: T,
    ) -> Result<Option<Block<TxHash>>, Self::Error> {
        let block_id = block_hash_or_number.into();
        self.before_event(TriggerType::GetBlock(block_id));
        // TODO change
        let r = Ok(Some(self.update_data(|data| data.best_block.clone())));
        self.after_event(TriggerType::GetBlock(block_id));
        r
    }

    fn get_logs_paginated<'a>(
        &'a self,
        filter: &Filter,
        page_size: u64,
    ) -> LogQuery<'a, Self::Provider> {
        LogQuery::new(Self::provider(&self), filter).with_page_size(page_size)
    }
}
