#![allow(clippy::arithmetic_side_effects)]

use crate::{
    new_service,
    ports::{
        BlockSigner,
        MockBlockImporter,
        MockBlockProducer,
        MockP2pPort,
        MockTransactionPool,
    },
    service::MainTask,
    signer::SignMode,
    Config,
    Service,
    Trigger,
};
use fuel_core_services::{
    stream::pending,
    Service as StorageTrait,
    State,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Consensus,
        header::BlockHeader,
        primitives::SecretKeyWrapper,
        SealedBlock,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        field::ScriptGasLimit,
        *,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    secrecy::Secret,
    services::executor::{
        Error as ExecutorError,
        ExecutionResult,
        UncommittedResult,
    },
    tai64::Tai64,
};
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    collections::HashSet,
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
    time,
};

mod manually_produce_tests;
mod trigger_tests;

struct TestContextBuilder {
    config: Option<Config>,
    txpool: Option<MockTransactionPool>,
    importer: Option<MockBlockImporter>,
    producer: Option<MockBlockProducer>,
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

    fn build(self) -> TestContext {
        let config = self.config.unwrap_or_default();
        let producer = self.producer.unwrap_or_else(|| {
            let mut producer = MockBlockProducer::default();
            producer
                .expect_produce_and_execute_block()
                .returning(|_, _, _| {
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

        let service = new_service(
            &BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now()),
            config.clone(),
            txpool,
            producer,
            importer,
            p2p_port,
            FakeBlockSigner(config.signer),
        );
        service.start().unwrap();
        TestContext { service }
    }
}

struct FakeBlockSigner(SignMode);

#[async_trait::async_trait]
impl BlockSigner for FakeBlockSigner {
    async fn seal_block(&self, block: &Block) -> anyhow::Result<Consensus> {
        self.0.seal_block(block).await
    }

    fn is_available(&self) -> bool {
        self.0.is_available()
    }
}

struct TestContext {
    service: Service<
        MockTransactionPool,
        MockBlockProducer,
        MockBlockImporter,
        FakeBlockSigner,
    >,
}

impl TestContext {
    async fn stop(&self) -> State {
        self.service.stop_and_await().await.unwrap()
    }
}

pub struct TxPoolContext {
    pub txpool: MockTransactionPool,
    pub txs: Arc<Mutex<Vec<Script>>>,
    pub status_sender: Arc<watch::Sender<Option<TxId>>>,
}

impl MockTransactionPool {
    fn no_tx_updates() -> Self {
        let mut txpool = MockTransactionPool::default();
        txpool
            .expect_transaction_status_events()
            .returning(|| Box::pin(pending()));
        txpool
    }

    pub fn new_with_txs(txs: Vec<Script>) -> TxPoolContext {
        let mut txpool = MockTransactionPool::default();
        let txs = Arc::new(StdMutex::new(txs));
        let (status_sender, status_receiver) = watch::channel(None);
        let status_sender = Arc::new(status_sender);
        let status_sender_clone = status_sender.clone();

        txpool
            .expect_transaction_status_events()
            .returning(move || {
                let status_channel =
                    (status_sender_clone.clone(), status_receiver.clone());
                let stream = fuel_core_services::stream::unfold(
                    status_channel,
                    |(sender, mut receiver)| async {
                        loop {
                            let status = *receiver.borrow_and_update();
                            if let Some(status) = status {
                                sender.send_replace(None);
                                return Some((status, (sender, receiver)))
                            }
                            receiver.changed().await.unwrap();
                        }
                    },
                );
                Box::pin(stream)
            });

        let pending = txs.clone();
        txpool
            .expect_pending_number()
            .returning(move || pending.lock().unwrap().len());
        let consumable = txs.clone();
        txpool.expect_total_consumable_gas().returning(move || {
            consumable
                .lock()
                .unwrap()
                .iter()
                .map(|tx| *tx.script_gas_limit())
                .sum()
        });
        let removed = txs.clone();
        txpool.expect_remove_txs().returning(
            move |tx_ids: Vec<(TxId, ExecutorError)>| {
                let mut guard = removed.lock().unwrap();
                for (id, _) in tx_ids {
                    guard.retain(|tx| tx.id(&ChainId::default()) == id);
                }
                vec![]
            },
        );

        TxPoolContext {
            txpool,
            txs,
            status_sender,
        }
    }
}

fn make_tx(rng: &mut StdRng) -> Script {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(0)
        .script_gas_limit(rng.gen_range(1..TxParameters::DEFAULT.max_gas_per_tx()))
        .finalize_without_signature()
}

#[tokio::test]
async fn remove_skipped_transactions() {
    // The test verifies that if `BlockProducer` returns skipped transactions, they would
    // be propagated to `TxPool` for removal.
    let mut rng = StdRng::seed_from_u64(2322);
    let secret_key = SecretKey::random(&mut rng);

    const TX_NUM: usize = 100;
    let skipped_transactions: Vec<_> = (0..TX_NUM).map(|_| make_tx(&mut rng)).collect();

    let mock_skipped_txs = skipped_transactions.clone();

    let mut block_producer = MockBlockProducer::default();
    block_producer
        .expect_produce_and_execute_block()
        .times(1)
        .returning(move |_, _, _| {
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block: Default::default(),
                    skipped_transactions: mock_skipped_txs
                        .clone()
                        .into_iter()
                        .map(|tx| {
                            (
                                tx.id(&ChainId::default()),
                                ExecutorError::OutputAlreadyExists,
                            )
                        })
                        .collect(),
                    tx_status: Default::default(),
                    events: Default::default(),
                },
                Default::default(),
            ))
        });

    let mut block_importer = MockBlockImporter::default();

    block_importer
        .expect_commit_result()
        .times(1)
        .returning(|_| Ok(()));

    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));

    let mut txpool = MockTransactionPool::no_tx_updates();
    // Test created for only for this check.
    txpool.expect_remove_txs().returning(move |skipped_ids| {
        let skipped_ids: Vec<_> = skipped_ids.into_iter().map(|(id, _)| id).collect();
        // Transform transactions into ids.
        let skipped_transactions: Vec<_> = skipped_transactions
            .iter()
            .map(|tx| tx.id(&ChainId::default()))
            .collect();

        // Check that all transactions are unique.
        let expected_skipped_ids_set: HashSet<_> =
            skipped_transactions.clone().into_iter().collect();
        assert_eq!(expected_skipped_ids_set.len(), TX_NUM);

        // Check that `TxPool::remove_txs` was called with the same ids in the same order.
        assert_eq!(skipped_ids.len(), TX_NUM);
        assert_eq!(skipped_transactions.len(), TX_NUM);
        assert_eq!(skipped_transactions, skipped_ids);
        vec![]
    });

    let signer = SignMode::Key(Secret::new(secret_key.into()));

    let config = Config {
        trigger: Trigger::Instant,
        signer: signer.clone(),
        metrics: false,
        ..Default::default()
    };

    let p2p_port = generate_p2p_port();

    let mut task = MainTask::new(
        &BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now()),
        config,
        txpool,
        block_producer,
        block_importer,
        p2p_port,
        FakeBlockSigner(signer),
    );

    assert!(task.produce_next_block().await.is_ok());
}

#[tokio::test]
async fn does_not_produce_when_txpool_empty_in_instant_mode() {
    // verify the PoA service doesn't trigger empty blocks to be produced when there are
    // irrelevant updates from the txpool
    let mut rng = StdRng::seed_from_u64(2322);
    let secret_key = SecretKey::random(&mut rng);

    let mut block_producer = MockBlockProducer::default();

    block_producer
        .expect_produce_and_execute_block()
        .returning(|_, _, _| panic!("Block production should not be called"));

    let mut block_importer = MockBlockImporter::default();

    block_importer
        .expect_commit_result()
        .returning(|_| panic!("Block importer should not be called"));
    block_importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));

    let mut txpool = MockTransactionPool::no_tx_updates();
    txpool.expect_total_consumable_gas().returning(|| 0);
    txpool.expect_pending_number().returning(|| 0);

    let signer = SignMode::Key(Secret::new(secret_key.into()));

    let config = Config {
        trigger: Trigger::Instant,
        signer: signer.clone(),
        metrics: false,
        ..Default::default()
    };

    let p2p_port = generate_p2p_port();

    let mut task = MainTask::new(
        &BlockHeader::new_block(BlockHeight::from(1u32), Tai64::now()),
        config,
        txpool,
        block_producer,
        block_importer,
        p2p_port,
        FakeBlockSigner(signer),
    );

    // simulate some txpool event to see if any block production is erroneously triggered
    task.on_txpool_event().await.unwrap();
}

fn test_signing_key() -> Secret<SecretKeyWrapper> {
    let mut rng = StdRng::seed_from_u64(0);
    let secret_key = SecretKey::random(&mut rng);
    Secret::new(secret_key.into())
}
