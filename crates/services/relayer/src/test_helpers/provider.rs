use alloy_consensus::{
    Signed,
    TxEip4844,
    transaction::Recovered,
};
use alloy_primitives::{
    Address,
    BlockNumber,
    Signature,
    TxHash,
    U64,
};
use alloy_provider::{
    DynProvider,
    EthGetBlock,
    Provider,
    ProviderBuilder,
    ProviderCall,
    RootProvider,
    mock::Asserter,
    network::Ethereum,
    transport::TransportResult,
};
use alloy_rpc_client::NoParams;
use alloy_rpc_types_eth::{
    Block,
    BlockId,
    Filter,
    Header,
    Log,
    SyncStatus,
    Transaction,
    TransactionReceipt,
};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::{
    fmt,
    fmt::Debug,
    sync::Arc,
};

pub type EventFn = Box<dyn for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync>;
pub type OverrideFn = Box<dyn FnMut(&mut MockData) + Send + Sync>;

#[derive(Default)]
struct InnerState {
    data: MockData,
    override_fn: Option<OverrideFn>,
}

#[derive(Debug)]
pub struct MockData {
    pub is_syncing: SyncStatus,
    pub best_block: Block<TxHash>,
    pub logs_batch: Vec<Vec<Log>>,
    _logs_batch_index: usize,
}

impl Default for MockData {
    fn default() -> Self {
        let header = Header {
            hash: "0xa1ea3121940930f7e7b54506d80717f14c5163807951624c36354202a8bffda6"
                .parse()
                .unwrap(),
            inner: alloy_consensus::Header {
                number: 20,
                ..Default::default()
            },
            ..Default::default()
        };

        let best_block = Block {
            header,
            ..Default::default()
        };

        MockData {
            best_block,
            is_syncing: SyncStatus::None,
            logs_batch: Vec::new(),
            _logs_batch_index: 0,
        }
    }
}

#[derive(Clone)]
pub struct MockProvider {
    inner: Box<DynProvider>,
    asserter: Asserter,
    data: Arc<Mutex<InnerState>>,
    before_event: Arc<Mutex<Option<EventFn>>>,
    after_event: Arc<Mutex<Option<EventFn>>>,
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

impl MockProvider {
    pub fn new(asserter: Asserter) -> Self {
        let provider = alloy_provider::ProviderBuilder::new()
            .connect_mocked_client(asserter.clone())
            .erased();
        Self {
            inner: Box::new(provider),
            asserter,
            ..Default::default()
        }
    }

    fn before_event(&self, trigger: TriggerType<'_>) {
        let mut be = self.before_event.lock();
        if let Some(be) = be.as_mut() {
            self.update_data(|data| be(data, trigger))
        }
    }

    fn after_event(&self, trigger: TriggerType<'_>) {
        let mut ae = self.after_event.lock();
        if let Some(ae) = ae.as_mut() {
            self.update_data(|data| ae(data, trigger))
        }
    }

    pub fn update_data<R>(&self, delta: impl FnOnce(&mut MockData) -> R) -> R {
        self.data.lock().update(delta)
    }

    /// Set a callback before an event.
    pub fn set_before_event(
        &self,
        f: impl for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync + 'static,
    ) {
        *self.before_event.lock() = Some(Box::new(f));
    }

    /// Set a callback after an event.
    pub fn set_after_event(
        &self,
        f: impl for<'a> FnMut(&mut MockData, TriggerType<'a>) + Send + Sync + 'static,
    ) {
        *self.after_event.lock() = Some(Box::new(f));
    }

    /// Set a callback to override the state any time the state is changed.
    pub fn set_state_override(
        &self,
        f: impl FnMut(&mut MockData) + Send + Sync + 'static,
    ) {
        self.data.lock().override_fn = Some(Box::new(f));
    }
}

async fn simulate_network_latency() {
    tokio::task::yield_now().await;
}

#[async_trait]
impl Provider for MockProvider {
    fn root(&self) -> &RootProvider<Ethereum> {
        unreachable!()
    }

    fn get_block_number(
        &self,
    ) -> ProviderCall<alloy_rpc_client::NoParams, U64, BlockNumber> {
        let this = self;
        this.before_event(TriggerType::GetBlockNumber);
        let r = self.update_data(|data| data.best_block.number());
        self.after_event(TriggerType::GetBlockNumber);
        self.asserter.push_success(&r);
        self.inner.get_block_number()
    }

    fn get_block(
        &self,
        block: BlockId,
    ) -> EthGetBlock<<Ethereum as alloy_provider::Network>::BlockResponse> {
        let cloned = self.clone();
        EthGetBlock::new_provider(
            block,
            Box::new(move |_| {
                let cloned = cloned.clone();
                ProviderCall::BoxedFuture(Box::pin(async move {
                    simulate_network_latency().await;
                    cloned.before_event(TriggerType::GetBlock(block));
                    let r = Some(cloned.update_data(|data| data.best_block.clone()));
                    cloned.after_event(TriggerType::GetBlock(block));
                    cloned.asserter.push_success(&r);
                    cloned.inner.get_block(block).await
                }))
            }),
        )
    }

    async fn get_logs(&self, filter: &Filter) -> TransportResult<Vec<Log>> {
        simulate_network_latency().await;
        self.before_event(TriggerType::GetLogs(filter));
        let r =
            self.update_data(|data| take_logs_based_on_filter(&data.logs_batch, filter));
        self.after_event(TriggerType::GetLogs(filter));
        self.asserter.push_success(&r);
        self.inner.get_logs(filter).await
    }

    fn get_transaction_by_hash(
        &self,
        hash: TxHash,
    ) -> ProviderCall<
        (TxHash,),
        Option<<Ethereum as alloy_provider::Network>::TransactionResponse>,
    > {
        let block_number = Some(self.update_data(|data| data.best_block.number()));
        let cloned_hash = hash;
        let cloned = self.clone();

        ProviderCall::BoxedFuture(Box::pin(async move {
            simulate_network_latency().await;
            let envelope = TxEip4844::default();
            let signer = Address::ZERO;
            let inner = Recovered::new_unchecked(envelope, signer);
            let txn = Transaction {
                block_number,
                transaction_index: None,
                inner,
                block_hash: Some(cloned_hash),
                effective_gas_price: None,
            };

            cloned.asserter.push_success(&txn);
            cloned.inner.get_transaction_by_hash(cloned_hash).await
        }))
    }

    fn get_transaction_receipt(
        &self,
        hash: TxHash,
    ) -> ProviderCall<
        (TxHash,),
        Option<<Ethereum as alloy_provider::Network>::ReceiptResponse>,
    > {
        let sig = Signature::test_signature();
        let tx = alloy_consensus::TxEnvelope::Legacy(Signed::new_unchecked(
            alloy_consensus::TxLegacy::default(),
            sig,
            Default::default(),
        ));
        let txn = TransactionReceipt {
            inner: tx,
            transaction_hash: Default::default(),
            transaction_index: None,
            block_hash: None,
            block_number: self.update_data(|data| {
                data.best_block.header.number = data.best_block.number() + 1u64;
                Some(data.best_block.number())
            }),

            gas_used: 0,
            effective_gas_price: 0,
            blob_gas_used: None,
            blob_gas_price: None,
            from: Default::default(),
            to: None,
            contract_address: None,
        };
        self.asserter.push_success(&txn);
        self.inner.get_transaction_receipt(hash)
    }

    fn syncing(&self) -> ProviderCall<NoParams, SyncStatus> {
        let cloned = self.clone();
        ProviderCall::BoxedFuture(Box::pin(async move {
            simulate_network_latency().await;
            cloned.before_event(TriggerType::Syncing);
            let r = cloned.update_data(|data| data.is_syncing.clone());
            cloned.after_event(TriggerType::Syncing);
            cloned.asserter.push_success(&r);
            cloned.inner.syncing().await
        }))
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        // Instantiates the nonce manager with `0` as nonce. The `address` should be the
        // address that you'll be sending transactions from
        let asserter = Asserter::new();
        Self {
            inner: Box::new(
                ProviderBuilder::new()
                    .connect_mocked_client(asserter.clone())
                    .erased(),
            ),
            asserter,
            data: Arc::new(Mutex::new(InnerState::default())),
            before_event: Arc::new(Mutex::new(None)),
            after_event: Arc::new(Mutex::new(None)),
        }
    }
}

impl fmt::Debug for MockProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockProvider")
            .field("data", &self.data)
            .finish()
    }
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

fn take_logs_based_on_filter(logs_batch: &[Vec<Log>], filter: &Filter) -> Vec<Log> {
    logs_batch
        .iter()
        .flatten()
        .filter(|log| {
            filter.matches_address(log.address())
                && filter.matches_block_range(log.block_number.unwrap())
        })
        .cloned()
        .collect()
}
