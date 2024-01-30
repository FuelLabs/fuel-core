use crate::{
    block_producer::Error,
    mocks::{
        FailingMockExecutor,
        MockDb,
        MockExecutor,
        MockRelayer,
        MockTxPool,
    },
    Config,
    Producer,
};
use fuel_core_producer as _;
use fuel_core_types::{
    blockchain::{
        block::{
            CompressedBlock,
            PartialFuelBlock,
        },
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
    },
    services::executor::Error as ExecutorError,
    tai64::Tai64,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::sync::{
    Arc,
    Mutex,
};

#[tokio::test]
async fn cant_produce_at_genesis_height() {
    let ctx = TestContext::default();
    let producer = ctx.producer();

    let err = producer
        .produce_and_execute_block_txpool(0u32.into(), Tai64::now(), 1_000_000_000)
        .await
        .expect_err("expected failure");

    assert!(
        matches!(err.downcast_ref::<Error>(), Some(Error::GenesisBlock)),
        "unexpected err {err:?}"
    );
}

#[tokio::test]
async fn can_produce_initial_block() {
    let ctx = TestContext::default();
    let producer = ctx.producer();

    let result = producer
        .produce_and_execute_block_txpool(1u32.into(), Tai64::now(), 1_000_000_000)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn can_produce_next_block() {
    // simple happy path for producing atop pre-existing block
    let mut rng = StdRng::seed_from_u64(0u64);
    // setup dummy previous block
    let prev_height = 1u32.into();
    let previous_block = PartialFuelBlock {
        header: PartialBlockHeader {
            consensus: ConsensusHeader {
                height: prev_height,
                prev_root: rng.gen(),
                ..Default::default()
            },
            ..Default::default()
        },
        transactions: vec![],
    }
    .generate(&[])
    .compress(&Default::default());

    let db = MockDb {
        blocks: Arc::new(Mutex::new(
            vec![(prev_height, previous_block)].into_iter().collect(),
        )),
    };

    let ctx = TestContext::default_from_db(db);
    let producer = ctx.producer();
    let result = producer
        .produce_and_execute_block_txpool(
            prev_height
                .succ()
                .expect("The block height should be valid"),
            Tai64::now(),
            1_000_000_000,
        )
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn cant_produce_if_no_previous_block() {
    // fail if there is no block that precedes the current height.
    let ctx = TestContext::default();
    let producer = ctx.producer();

    let err = producer
        .produce_and_execute_block_txpool(100u32.into(), Tai64::now(), 1_000_000_000)
        .await
        .expect_err("expected failure");

    assert!(err.to_string().contains("Didn't find block for test"));
}

#[tokio::test]
async fn cant_produce_if_previous_block_da_height_too_high() {
    // setup previous block with a high da_height
    let prev_da_height = 100u64.into();
    let prev_height = 1u32.into();
    let previous_block = PartialFuelBlock {
        header: PartialBlockHeader {
            application: ApplicationHeader {
                da_height: prev_da_height,
                ..Default::default()
            },
            consensus: ConsensusHeader {
                height: prev_height,
                ..Default::default()
            },
        },
        transactions: vec![],
    }
    .generate(&[])
    .compress(&Default::default());

    let db = MockDb {
        blocks: Arc::new(Mutex::new(
            vec![(prev_height, previous_block)].into_iter().collect(),
        )),
    };
    let ctx = TestContext {
        relayer: MockRelayer {
            // set our relayer best finalized height to less than previous
            best_finalized_height: prev_da_height - 1u64.into(),
            ..Default::default()
        },
        ..TestContext::default_from_db(db)
    };
    let producer = ctx.producer();

    let err = producer
        .produce_and_execute_block_txpool(
            prev_height
                .succ()
                .expect("The block height should be valid"),
            Tai64::now(),
            1_000_000_000,
        )
        .await
        .expect_err("expected failure");

    assert!(
        matches!(
            err.downcast_ref::<Error>(),
            Some(Error::InvalidDaFinalizationState {
                previous_block,
                best
            }) if *previous_block == prev_da_height && *best == prev_da_height - 1u64.into()
        ),
        "unexpected err {err:?}"
    );
}

#[tokio::test]
async fn production_fails_on_execution_error() {
    let ctx = TestContext::default_from_executor(FailingMockExecutor(Mutex::new(Some(
        ExecutorError::TransactionIdCollision(Default::default()),
    ))));

    let producer = ctx.producer();

    let err = producer
        .produce_and_execute_block_txpool(1u32.into(), Tai64::now(), 1_000_000_000)
        .await
        .expect_err("expected failure");

    assert!(
        matches!(
            err.downcast_ref::<ExecutorError>(),
            Some(ExecutorError::TransactionIdCollision { .. })
        ),
        "unexpected err {err:?}"
    );
}

struct TestContext<Executor> {
    config: Config,
    db: MockDb,
    relayer: MockRelayer,
    executor: Arc<Executor>,
    txpool: MockTxPool,
}

impl TestContext<MockExecutor> {
    pub fn default() -> Self {
        Self::default_from_db(Self::default_db())
    }

    pub fn default_from_db(db: MockDb) -> Self {
        let executor = MockExecutor(db.clone());
        Self::default_from_db_and_executor(db, executor)
    }
}

impl<Executor> TestContext<Executor> {
    fn default_db() -> MockDb {
        let genesis_height = 0u32.into();
        let genesis_block = CompressedBlock::default();

        MockDb {
            blocks: Arc::new(Mutex::new(
                vec![(genesis_height, genesis_block)].into_iter().collect(),
            )),
        }
    }

    pub fn default_from_executor(executor: Executor) -> Self {
        Self::default_from_db_and_executor(Self::default_db(), executor)
    }

    pub fn default_from_db_and_executor(db: MockDb, executor: Executor) -> Self {
        let txpool = MockTxPool::default();
        let relayer = MockRelayer::default();
        let config = Config::default();
        Self {
            config,
            db,
            relayer,
            executor: Arc::new(executor),
            txpool,
        }
    }

    pub fn producer(self) -> Producer<MockDb, MockTxPool, Executor> {
        Producer {
            config: self.config,
            view_provider: self.db,
            txpool: self.txpool,
            executor: self.executor,
            relayer: Box::new(self.relayer),
            lock: Default::default(),
        }
    }
}
