#![allow(clippy::arithmetic_side_effects)]
#![allow(non_snake_case)]

use crate::{
    Config,
    Service,
    Trigger,
    new_service,
    ports::{
        BlockProducer,
        BlockSigner,
        GetTime,
        InMemoryPredefinedBlocks,
        MockBlockImporter,
        MockBlockProducer,
        MockP2pPort,
        MockTransactionPool,
        TransactionsSource,
        WaitForReadySignal,
    },
    service::MainTask,
};
use fuel_core_chain_config::default_consensus_dev_key;
use fuel_core_services::{
    Service as StorageTrait,
    ServiceRunner,
    State,
};
use fuel_core_storage::transactional::Changes;
use fuel_core_types::{
    blockchain::{
        SealedBlock,
        block::Block,
        consensus::{
            Consensus,
            poa::PoAConsensus,
        },
        header::{
            BlockHeader,
            PartialBlockHeader,
        },
        primitives::SecretKeyWrapper,
    },
    fuel_crypto::SecretKey,
    fuel_tx::*,
    fuel_types::BlockHeight,
    secrecy::Secret,
    services::executor::{
        ExecutionResult,
        UncommittedResult,
    },
    signer::SignMode,
    tai64::{
        Tai64,
        Tai64N,
    },
};
use rand::{
    Rng,
    SeedableRng,
    prelude::StdRng,
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex as StdMutex,
        Mutex,
    },
    time::Duration,
};
use tokio::{
    sync::{
        broadcast,
        watch,
    },
    time::{
        self,
        Instant,
    },
};

mod manually_produce_tests;
mod test_time;
mod trigger_tests;
use test_time::TestTime;

struct TestContextBuilder {
    config: Option<Config>,
    txpool: Option<MockTransactionPool>,
    importer: Option<MockBlockImporter>,
    producer: Option<MockBlockProducer>,
    start_time: Option<Tai64N>,
}

fn generate_p2p_port() -> MockP2pPort {
    let mut p2p_port = MockP2pPort::default();

    p2p_port
        .expect_reserved_peers_count()
        .returning(move || Box::pin(tokio_stream::pending()));

    p2p_port
}

impl TestContextBuilder {
    fn new() -> Self {
        Self {
            config: None,
            txpool: None,
            importer: None,
            producer: None,
            start_time: None,
        }
    }

    fn with_config(&mut self, config: Config) -> &mut Self {
        self.config = Some(config);
        self
    }

    fn with_txpool(&mut self, txpool: MockTransactionPool) -> &mut Self {
        self.txpool = Some(txpool);
        self
    }

    fn with_importer(&mut self, importer: MockBlockImporter) -> &mut Self {
        self.importer = Some(importer);
        self
    }

    fn with_producer(&mut self, producer: MockBlockProducer) -> &mut Self {
        self.producer = Some(producer);
        self
    }

    async fn build(self) -> TestContext {
        let config = self.config.unwrap_or_default();
        let producer = self.producer.unwrap_or_else(|| {
            let mut producer = MockBlockProducer::default();
            producer
                .expect_produce_and_execute_block()
                .returning(|_, _, _, _| {
                    Ok(UncommittedResult::new(
                        ExecutionResult {
                            block: Default::default(),
                            skipped_transactions: Default::default(),
                            tx_status: Default::default(),
                            events: Default::default(),
                        },
                        Default::default(),
                    ))
                });
            producer
        });

        let importer = self.importer.unwrap_or_else(|| {
            let mut importer = MockBlockImporter::default();
            importer.expect_commit_result().returning(|_| Ok(()));
            importer
                .expect_block_stream()
                .returning(|| Box::pin(tokio_stream::pending()));
            importer
        });

        let txpool = self
            .txpool
            .unwrap_or_else(MockTransactionPool::no_tx_updates);

        let p2p_port = generate_p2p_port();

        let predefined_blocks = HashMap::new().into();

        let time = self.start_time.map(TestTime::new).unwrap_or_default();

        let watch = time.watch();

        let block_production_ready_signal = FakeBlockProductionReadySignal;

        let service = new_service(
            &BlockHeader::new_block(BlockHeight::from(1u32), watch.now()),
            config.clone(),
            txpool,
            producer,
            importer,
            p2p_port,
            FakeBlockSigner { succeeds: true }.into(),
            predefined_blocks,
            watch,
            block_production_ready_signal.clone(),
        );

        service.start_and_await().await.unwrap();
        TestContext { service, time }
    }
}

pub struct FakeBlockSigner {
    succeeds: bool,
}

#[async_trait::async_trait]
impl BlockSigner for FakeBlockSigner {
    async fn seal_block(&self, block: &Block) -> anyhow::Result<Consensus> {
        if self.succeeds {
            let signature =
                SignMode::Key(Secret::new(default_consensus_dev_key().into()))
                    .sign_message(block.id().into_message())
                    .await?;

            Ok(Consensus::PoA(PoAConsensus::new(signature)))
        } else {
            Err(anyhow::anyhow!("failed to sign block"))
        }
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct FakeBlockProductionReadySignal;

impl WaitForReadySignal for FakeBlockProductionReadySignal {
    async fn wait_for_ready_signal(&self) {}
}

pub type TestPoAService = Service<
    MockBlockProducer,
    MockBlockImporter,
    FakeBlockSigner,
    InMemoryPredefinedBlocks,
    test_time::Watch,
    FakeBlockProductionReadySignal,
>;

struct TestContext {
    service: TestPoAService,
    time: TestTime,
}

impl TestContext {
    async fn stop(&self) -> State {
        self.service.stop_and_await().await.unwrap()
    }
}

pub struct TxPoolContext {
    pub txpool: MockTransactionPool,
    pub txs: Arc<Mutex<Vec<Script>>>,
    pub new_txs_notifier: watch::Sender<()>,
}

impl MockTransactionPool {
    fn no_tx_updates() -> Self {
        let mut txpool = MockTransactionPool::default();
        txpool.expect_new_txs_watcher().returning({
            let (sender, _) = watch::channel(());
            move || sender.subscribe()
        });
        txpool
    }

    pub fn new_with_txs(txs: Vec<Script>) -> TxPoolContext {
        let mut txpool = MockTransactionPool::default();
        let txs = Arc::new(StdMutex::new(txs));
        let (new_txs_notifier, _) = watch::channel(());

        txpool.expect_new_txs_watcher().returning({
            let sender = new_txs_notifier.clone();
            move || sender.subscribe()
        });

        TxPoolContext {
            txpool,
            txs,
            new_txs_notifier,
        }
    }
}

fn make_tx(rng: &mut StdRng) -> Script {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(0)
        .script_gas_limit(rng.gen_range(1..TxParameters::DEFAULT.max_gas_per_tx()))
        .finalize_without_signature()
}

fn test_signing_key() -> Secret<SecretKeyWrapper> {
    let mut rng = StdRng::seed_from_u64(0);
    let secret_key = SecretKey::random(&mut rng);
    Secret::new(secret_key.into())
}

#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
enum FakeProducedBlock {
    Predefined(Block),
    New(BlockHeight, Tai64),
}

struct FakeBlockProducer {
    block_sender: tokio::sync::mpsc::Sender<FakeProducedBlock>,
}

impl FakeBlockProducer {
    fn new() -> (Self, tokio::sync::mpsc::Receiver<FakeProducedBlock>) {
        let (block_sender, receiver) = tokio::sync::mpsc::channel(100);
        (Self { block_sender }, receiver)
    }
}

#[async_trait::async_trait]
impl BlockProducer for FakeBlockProducer {
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        block_time: Tai64,
        _: TransactionsSource,
        _: Instant,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        self.block_sender
            .send(FakeProducedBlock::New(height, block_time))
            .await
            .unwrap();
        Ok(UncommittedResult::new(
            ExecutionResult {
                block: Default::default(),
                skipped_transactions: Default::default(),
                tx_status: Default::default(),
                events: Default::default(),
            },
            Default::default(),
        ))
    }

    async fn produce_predefined_block(
        &self,
        block: &Block,
    ) -> anyhow::Result<UncommittedResult<Changes>> {
        self.block_sender
            .send(FakeProducedBlock::Predefined(block.clone()))
            .await
            .unwrap();
        Ok(UncommittedResult::new(
            ExecutionResult {
                block: block.clone(),
                skipped_transactions: Default::default(),
                tx_status: Default::default(),
                events: Default::default(),
            },
            Default::default(),
        ))
    }
}

fn block_for_height(height: u32) -> Block {
    let mut header = PartialBlockHeader::default();
    header.consensus.height = height.into();
    let transactions = vec![];
    Block::new(
        header,
        transactions,
        Default::default(),
        Default::default(),
        #[cfg(feature = "fault-proving")]
        &Default::default(),
    )
    .unwrap()
}

#[tokio::test]
async fn consensus_service__run__will_include_sequential_predefined_blocks_before_new_blocks()
 {
    // given
    let blocks: [(BlockHeight, Block); 3] = [
        (2u32.into(), block_for_height(2)),
        (3u32.into(), block_for_height(3)),
        (4u32.into(), block_for_height(4)),
    ];
    let signer = SignMode::Key(test_signing_key());
    let blocks_map: HashMap<_, _> = blocks.clone().into_iter().collect();
    let (block_producer, mut block_receiver) = FakeBlockProducer::new();
    let last_block = BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: Duration::from_millis(100),
        },
        signer,
        metrics: false,
        ..Default::default()
    };

    let mut block_importer = MockBlockImporter::default();
    block_importer.expect_commit_result().returning(|_| Ok(()));
    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::empty()));
    let mut rng = StdRng::seed_from_u64(0);
    let tx = make_tx(&mut rng);
    let TxPoolContext { txpool, .. } = MockTransactionPool::new_with_txs(vec![tx]);
    let time = TestTime::at_unix_epoch();
    let block_production_ready_signal = FakeBlockProductionReadySignal;

    let task = MainTask::new(
        &last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        generate_p2p_port(),
        FakeBlockSigner { succeeds: true }.into(),
        InMemoryPredefinedBlocks::new(blocks_map),
        time.watch(),
        block_production_ready_signal.clone(),
    );

    // when
    let service = ServiceRunner::new(task);
    service.start().unwrap();

    // then
    for (_, block) in blocks {
        let expected = FakeProducedBlock::Predefined(block);
        let actual = block_receiver.recv().await.unwrap();
        assert_eq!(expected, actual);
    }
    let maybe_produced_block = block_receiver.recv().await.unwrap();
    assert!(matches! {
        maybe_produced_block,
        FakeProducedBlock::New(_, _)
    });
}

#[tokio::test]
async fn consensus_service__run__will_insert_predefined_blocks_in_correct_order() {
    // given
    let predefined_blocks: &[Option<(BlockHeight, Block)>] = &[
        None,
        Some((3u32.into(), block_for_height(3))),
        None,
        Some((5u32.into(), block_for_height(5))),
        None,
        Some((7u32.into(), block_for_height(7))),
        None,
    ];
    let predefined_blocks_map: HashMap<_, _> = predefined_blocks
        .iter()
        .flat_map(|x| x.to_owned())
        .collect();
    let (block_producer, mut block_receiver) = FakeBlockProducer::new();
    let last_block = BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: Duration::from_millis(100),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    };
    let mut block_importer = MockBlockImporter::default();
    block_importer.expect_commit_result().returning(|_| Ok(()));
    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::empty()));
    let mut rng = StdRng::seed_from_u64(0);
    let tx = make_tx(&mut rng);
    let TxPoolContext { txpool, .. } = MockTransactionPool::new_with_txs(vec![tx]);
    let time = TestTime::at_unix_epoch();
    let block_production_ready_signal = FakeBlockProductionReadySignal;

    let task = MainTask::new(
        &last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        generate_p2p_port(),
        FakeBlockSigner { succeeds: true }.into(),
        InMemoryPredefinedBlocks::new(predefined_blocks_map),
        time.watch(),
        block_production_ready_signal.clone(),
    );

    // when
    let service = ServiceRunner::new(task);
    service.start().unwrap();

    // then
    for maybe_predefined in predefined_blocks {
        let actual = block_receiver.recv().await.unwrap();
        if let Some((_, block)) = maybe_predefined {
            let expected = FakeProducedBlock::Predefined(block.clone());
            assert_eq!(expected, actual);
        } else {
            assert!(matches! {
                actual,
                FakeProducedBlock::New(_, _)
            });
        }
    }
}

#[derive(Clone)]
struct MockBlockProductionReadySignal(std::sync::Arc<tokio::sync::Notify>);

impl WaitForReadySignal for MockBlockProductionReadySignal {
    async fn wait_for_ready_signal(&self) {
        self.0.notified().await;
    }
}

impl Default for MockBlockProductionReadySignal {
    fn default() -> Self {
        Self(std::sync::Arc::new(tokio::sync::Notify::new()))
    }
}

impl MockBlockProductionReadySignal {
    fn send_ready_signal(&self) {
        self.0.notify_one();
    }
}

#[tokio::test]
async fn consensus_service__run__will_not_produce_blocks_without_ready_signal() {
    // this test basically checks that the block production ready signal is working
    // no blocks should be produced if the ready signal is not sent

    // given
    let (block_producer, mut block_receiver) = FakeBlockProducer::new();
    let last_block = BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: std::time::Duration::from_millis(10),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    };
    let mut block_importer = MockBlockImporter::default();
    block_importer.expect_commit_result().returning(|_| Ok(()));
    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::empty()));
    let mut rng = StdRng::seed_from_u64(0);
    let tx = make_tx(&mut rng);
    let TxPoolContext { txpool, .. } = MockTransactionPool::new_with_txs(vec![tx]);
    let time = TestTime::at_unix_epoch();
    let block_production_ready_signal = MockBlockProductionReadySignal::default();

    let task = MainTask::new(
        &last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        generate_p2p_port(),
        FakeBlockSigner { succeeds: true }.into(),
        InMemoryPredefinedBlocks::new(HashMap::new()),
        time.watch(),
        block_production_ready_signal.clone(),
    );

    // when
    let service = ServiceRunner::new(task);
    service.start_and_await().await.unwrap();
    // we don't send the ready signal

    // then
    time::sleep(Duration::from_millis(200)).await;
    assert!(block_receiver.try_recv().is_err());
}

#[tokio::test]
async fn consensus_service__run__will_produce_blocks_with_ready_signal() {
    // given
    let (block_producer, mut block_receiver) = FakeBlockProducer::new();
    let last_block = BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: std::time::Duration::from_millis(10),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    };
    let mut block_importer = MockBlockImporter::default();
    block_importer.expect_commit_result().returning(|_| Ok(()));
    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::empty()));
    let mut rng = StdRng::seed_from_u64(0);
    let tx = make_tx(&mut rng);
    let TxPoolContext { txpool, .. } = MockTransactionPool::new_with_txs(vec![tx]);
    let time = TestTime::at_unix_epoch();
    let block_production_ready_signal = MockBlockProductionReadySignal::default();

    let task = MainTask::new(
        &last_block,
        config,
        txpool,
        block_producer,
        block_importer,
        generate_p2p_port(),
        FakeBlockSigner { succeeds: true }.into(),
        InMemoryPredefinedBlocks::new(HashMap::new()),
        time.watch(),
        block_production_ready_signal.clone(),
    );

    // when
    let service = ServiceRunner::new(task);
    service.start_and_await().await.unwrap();
    block_production_ready_signal.send_ready_signal();

    // then
    let produced_block = block_receiver.recv().await.unwrap();
    assert!(matches!(produced_block, FakeProducedBlock::New(_, _)));
}
