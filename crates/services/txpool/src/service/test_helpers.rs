use super::*;
use crate::{
    mock_db::MockDBProvider,
    ports::{
        BlockImporter,
        MockConsensusParametersProvider,
    },
    test_helpers::MockWasmChecker,
    types::GasPrice,
    MockDb,
    Result as TxPoolResult,
};
use fuel_core_services::{
    stream::BoxStream,
    Service as ServiceTrait,
};
use fuel_core_types::{
    blockchain::SealedBlock,
    entities::coins::coin::Coin,
    fuel_crypto::rand::{
        rngs::StdRng,
        SeedableRng,
    },
    fuel_tx::{
        Cacheable,
        Input,
        Transaction,
        TransactionBuilder,
        Word,
    },
    fuel_vm::interpreter::MemoryInstance,
    services::{
        block_importer::ImportResult,
        p2p::GossipsubMessageAcceptance,
    },
};
use std::cell::RefCell;

type GossipedTransaction = GossipData<Transaction>;

pub struct DummyPool;

#[async_trait::async_trait]
impl MemoryPool for DummyPool {
    type Memory = MemoryInstance;

    async fn get_memory(&self) -> Self::Memory {
        MemoryInstance::new()
    }
}

pub struct TestContext {
    pub(crate) service: Service<
        MockP2P,
        MockDBProvider,
        MockWasmChecker,
        MockTxPoolGasPrice,
        MockConsensusParametersProvider,
        DummyPool,
    >,
    mock_db: MockDb,
    rng: RefCell<StdRng>,
}

#[derive(Debug, Clone)]
pub struct MockTxPoolGasPrice {
    pub gas_price: Option<GasPrice>,
}

impl MockTxPoolGasPrice {
    pub fn new(gas_price: GasPrice) -> Self {
        Self {
            gas_price: Some(gas_price),
        }
    }

    pub fn new_none() -> Self {
        Self { gas_price: None }
    }
}

#[async_trait::async_trait]
impl GasPriceProviderConstraint for MockTxPoolGasPrice {
    async fn next_gas_price(&self) -> TxPoolResult<GasPrice> {
        self.gas_price.ok_or(TxPoolError::GasPriceNotFound(
            "Gas price not found".to_string(),
        ))
    }
}

impl TestContext {
    pub async fn new() -> Self {
        TestContextBuilder::new().build_and_start().await
    }

    pub fn service(
        &self,
    ) -> &Service<
        MockP2P,
        MockDBProvider,
        MockWasmChecker,
        MockTxPoolGasPrice,
        MockConsensusParametersProvider,
        DummyPool,
    > {
        &self.service
    }

    pub fn setup_script_tx(&self, tip: Word) -> Transaction {
        let (_, gas_coin) = self.setup_coin();
        let mut tx = TransactionBuilder::script(vec![], vec![])
            .max_fee_limit(tip)
            .tip(tip)
            .script_gas_limit(1000)
            .add_input(gas_coin)
            .finalize_as_transaction();

        tx.precompute(&Default::default())
            .expect("Should be able to cache");
        tx
    }

    pub fn setup_coin(&self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng.borrow_mut(), Some(&self.mock_db))
    }
}

mockall::mock! {
    pub P2P {}

    impl PeerToPeer for P2P {
        type GossipedTransaction = GossipedTransaction;

        fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

        fn gossiped_transaction_events(&self) -> BoxStream<GossipedTransaction>;

        fn notify_gossip_transaction_validity(
            &self,
            message_info: GossipsubMessageInfo,
            validity: GossipsubMessageAcceptance,
        ) -> anyhow::Result<()>;
    }
}

impl MockP2P {
    pub fn new_with_txs(txs: Vec<Transaction>) -> Self {
        let mut p2p = MockP2P::default();
        p2p.expect_gossiped_transaction_events().returning(move || {
            let txs_clone = txs.clone();
            let stream = fuel_core_services::stream::unfold(txs_clone, |mut txs| async {
                let tx = txs.pop();
                if let Some(tx) = tx {
                    Some((GossipData::new(tx, vec![], vec![]), txs))
                } else {
                    core::future::pending().await
                }
            });
            Box::pin(stream)
        });

        p2p
    }
}

mockall::mock! {
    pub Importer {}

    impl BlockImporter for Importer {
        fn block_events(&self) -> BoxStream<SharedImportResult>;
    }
}

impl MockImporter {
    fn with_blocks(blocks: Vec<SealedBlock>) -> Self {
        let mut importer = MockImporter::default();
        importer.expect_block_events().returning(move || {
            let blocks = blocks.clone();
            let stream = fuel_core_services::stream::unfold(blocks, |mut blocks| async {
                let block = blocks.pop();
                if let Some(sealed_block) = block {
                    let result: SharedImportResult = Arc::new(
                        ImportResult::new_from_local(sealed_block, vec![], vec![]),
                    );

                    Some((result, blocks))
                } else {
                    core::future::pending().await
                }
            });
            Box::pin(stream)
        });
        importer
    }
}

pub struct TestContextBuilder {
    config: Option<Config>,
    mock_db: MockDb,
    rng: StdRng,
    p2p: Option<MockP2P>,
    importer: Option<MockImporter>,
}

impl Default for TestContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestContextBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            mock_db: MockDb::default(),
            rng: StdRng::seed_from_u64(10),
            p2p: None,
            importer: None,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_importer(&mut self, importer: MockImporter) {
        self.importer = Some(importer)
    }

    pub fn with_p2p(&mut self, p2p: MockP2P) {
        self.p2p = Some(p2p)
    }

    pub fn setup_script_tx(&mut self, tip: Word) -> Transaction {
        let (_, gas_coin) = self.setup_coin();
        TransactionBuilder::script(vec![], vec![])
            .tip(tip)
            .max_fee_limit(tip)
            .script_gas_limit(1000)
            .add_input(gas_coin)
            .finalize_as_transaction()
    }

    pub fn setup_coin(&mut self) -> (Coin, Input) {
        crate::test_helpers::setup_coin(&mut self.rng, Some(&self.mock_db))
    }

    pub fn build(self) -> TestContext {
        let rng = RefCell::new(self.rng);
        let gas_price = 0;
        let config = self.config.unwrap_or_default();
        let mock_db = self.mock_db;

        let mut p2p = self.p2p.unwrap_or_else(|| MockP2P::new_with_txs(vec![]));
        // set default handlers for p2p methods after test is set up, so they will be last on the FIFO
        // ordering of methods handlers: https://docs.rs/mockall/0.12.1/mockall/index.html#matching-multiple-calls
        p2p.expect_notify_gossip_transaction_validity()
            .returning(move |_, _| Ok(()));
        p2p.expect_broadcast_transaction()
            .returning(move |_| Ok(()));

        let importer = self
            .importer
            .unwrap_or_else(|| MockImporter::with_blocks(vec![]));
        let gas_price_provider = MockTxPoolGasPrice::new(gas_price);
        let mut consensus_parameters_provider =
            MockConsensusParametersProvider::default();
        consensus_parameters_provider
            .expect_latest_consensus_parameters()
            .returning(|| (0, Arc::new(Default::default())));

        let service = new_service(
            config,
            MockDBProvider(mock_db.clone()),
            importer,
            p2p,
            MockWasmChecker { result: Ok(()) },
            Default::default(),
            gas_price_provider,
            consensus_parameters_provider,
            DummyPool,
        );

        TestContext {
            service,
            mock_db,
            rng,
        }
    }

    pub async fn build_and_start(self) -> TestContext {
        let context = self.build();
        context.service.start_and_await().await.unwrap();
        context
    }
}
