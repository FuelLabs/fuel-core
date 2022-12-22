use crate::{
    deadline_clock::DeadlineClock,
    new_service,
    service::PoA,
    Config,
    Service,
    Trigger,
};
use anyhow::anyhow;
use fuel_core_poa::{
    ports::{
        BlockDb,
        BlockProducer,
        TransactionPool,
    },
    service::new_service,
    Config,
    Trigger,
};
use fuel_core_services::{
    Service as StorageTrait,
    Service as ServiceTrait,
};
use fuel_core_storage::{
    transactional::{
        StorageTransaction,
        Transactional,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
        consensus::Consensus,
        header::{
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::{
            BlockHeight,
            BlockId,
            SecretKeyWrapper,
        },
        SealedBlock,
    },
    fuel_asm::*,
    fuel_crypto::SecretKey,
    fuel_tx::*,
    fuel_vm::consts::REG_ZERO,
    secrecy::{
        ExposeSecret,
        Secret,
    },
    services::{
        executor::{
            Error as ExecutorError,
            ExecutionResult,
            UncommittedResult,
        },
        txpool::{
            ArcPoolTx,
            Error as TxPoolError,
            PoolTransaction,
            TxStatus,
        },
    },
};
use rand::{
    prelude::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    cmp::Reverse,
    collections::{
        hash_map::Entry,
        HashMap,
        HashSet,
    },
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{
        broadcast,
        mpsc,
        oneshot,
        Mutex,
    },
    task::JoinHandle,
    time,
    time::Instant,
};

struct TestContextBuilder {}

type BoxFuture<'a, T> =
    core::pin::Pin<Box<dyn core::future::Future<Output = T> + Send + 'a>>;

mockall::mock! {
    TxPool {}

    #[async_trait::async_trait]
    impl TransactionPool for TxPool {
        fn pending_number(&self) -> usize;

        fn total_consumable_gas(&self) -> u64;

        fn remove_txs(&self, tx_ids: Vec<TxId>) -> Vec<ArcPoolTx>;

        fn next_transaction_status_update<'_self, 'a>(
            &'_self mut self,
        ) -> BoxFuture<'a, TxStatus>
        where
            '_self: 'a,
            Self: Sync + 'a;
    }
}

impl MockTxPool {
    pub fn no_tx_updates() -> Self {
        let mut txpool = MockTxPool::default();
        txpool
            .expect_next_transaction_status_update()
            .returning(|| Box::pin(async { core::future::pending::<TxStatus>().await }));
        txpool
    }
}

mockall::mock! {
    Database {}

    unsafe impl Sync for Database {}
    unsafe impl Send for Database {}

    impl BlockDb for Database {
        fn block_height(&self) -> anyhow::Result<BlockHeight>;

        fn seal_block(
            &mut self,
            block_id: BlockId,
            consensus: Consensus,
        ) -> anyhow::Result<()>;
    }

    impl Transactional<MockDatabase> for Database {
        fn commit(&mut self) -> StorageResult<()>;
    }

    impl AsRef<MockDatabase> for Database {
        fn as_ref(&self) -> &Self;
    }

    impl AsMut<MockDatabase> for Database {
        fn as_mut(&mut self) -> &mut Self;
    }
}

mockall::mock! {
    BlockProducer {}

    #[async_trait::async_trait]
    impl BlockProducer<MockDatabase> for BlockProducer {
        async fn produce_and_execute_block(
            &self,
            _height: BlockHeight,
            _max_gas: Word,
        ) -> anyhow::Result<UncommittedResult<StorageTransaction<MockDatabase>>>;

        async fn dry_run(
            &self,
            _transaction: Transaction,
            _height: Option<BlockHeight>,
            _utxo_validation: Option<bool>,
        ) -> anyhow::Result<Vec<Receipt>>;
    }
}

fn make_tx(rng: &mut StdRng) -> Transaction {
    TransactionBuilder::create(rng.gen(), rng.gen(), vec![])
        .gas_price(rng.gen())
        .gas_limit(rng.gen())
        .finalize_without_signature_as_transaction()
}

#[tokio::test]
async fn remove_skipped_transactions() {
    // The test verifies that if `BlockProducer` returns skipped transactions, they would
    // be propagated to `TxPool` for removal.
    let mut rng = StdRng::seed_from_u64(2322);
    let secret_key = SecretKey::random(&mut rng);

    let (import_block_events_tx, mut import_block_receiver_tx) = broadcast::channel(1);
    tokio::spawn(async move {
        import_block_receiver_tx.recv().await.unwrap();
    });

    const TX_NUM: usize = 100;
    let skipped_transactions: Vec<_> = (0..TX_NUM).map(|_| make_tx(&mut rng)).collect();

    let mock_skipped_txs = skipped_transactions.clone();

    let mut seq = mockall::Sequence::new();

    let mut block_producer = MockBlockProducer::default();
    block_producer
        .expect_produce_and_execute_block()
        .times(1)
        .in_sequence(&mut seq)
        .returning(move |_, _| {
            let mut db = MockDatabase::default();

            let mut db_inner = MockDatabase::default();
            // We expect that `seal_block` should be called 1 time after `produce_and_execute_block`.
            db_inner
                .expect_seal_block()
                .times(1)
                .in_sequence(&mut seq)
                .returning(|_, _| Ok(()));
            db
                .expect_commit()
                // Verifies that `commit` have been called.
                .times(1)
                .in_sequence(&mut seq)
                .returning(|| Ok(()));
            // Check that `commit` is called after `seal_block`.
            db.expect_as_mut().times(1).return_var(db_inner);

            Ok(UncommittedResult::new(
                ExecutionResult {
                    block: Default::default(),
                    skipped_transactions: mock_skipped_txs
                        .clone()
                        .into_iter()
                        .map(|tx| (tx, ExecutorError::OutputAlreadyExists))
                        .collect(),
                    tx_status: Default::default(),
                },
                StorageTransaction::new(db),
            ))
        });

    let mut db = MockDatabase::default();
    db.expect_block_height()
        .returning(|| Ok(BlockHeight::from(1u32)));

    let mut txpool = MockTxPool::default();
    // Test created for only for this check.
    txpool.expect_remove_txs().returning(move |skipped_ids| {
        // Transform transactions into ids.
        let skipped_transactions: Vec<_> =
            skipped_transactions.iter().map(|tx| tx.id()).collect();

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

    let mut task = PoA {
        block_gas_limit: 1000000,
        signing_key: Some(Secret::new(secret_key.into())),
        db,
        block_producer,
        txpool,
        import_block_events_tx,
        last_block_created: Instant::now(),
        trigger: Trigger::Instant,
        timer: DeadlineClock::new(),
    };

    assert!(task.produce_block().await.is_ok());
}

#[tokio::test]
async fn does_not_produce_when_txpool_empty_in_instant_mode() {
    // verify the PoA service doesn't trigger empty blocks to be produced when there are
    // irrelevant updates from the txpool
    let mut rng = StdRng::seed_from_u64(2322);
    let secret_key = SecretKey::random(&mut rng);

    let (import_block_events_tx, mut import_block_receiver_tx) = broadcast::channel(1);
    tokio::spawn(async move {
        import_block_receiver_tx.recv().await.unwrap();
    });

    let mut block_producer = MockBlockProducer::default();

    block_producer
        .expect_produce_and_execute_block()
        .returning(|_, _| panic!("Block production should not be called"));

    let mut db = MockDatabase::default();
    db.expect_block_height()
        .returning(|| Ok(BlockHeight::from(1u32)));

    let mut txpool = MockTxPool::default();
    txpool.expect_total_consumable_gas().returning(|| 0);
    txpool.expect_pending_number().returning(|| 0);

    let mut task = PoA {
        block_gas_limit: 1000000,
        signing_key: Some(Secret::new(secret_key.into())),
        db,
        block_producer,
        txpool,
        import_block_events_tx,
        last_block_created: Instant::now(),
        trigger: Trigger::Instant,
        timer: DeadlineClock::new(),
    };

    // simulate some txpool events to see if any block production is erroneously triggered
    task.on_txpool_event(&TxStatus::Submitted).await.unwrap();
    task.on_txpool_event(&TxStatus::Completed).await.unwrap();
    task.on_txpool_event(&TxStatus::SqueezedOut {
        reason: TxPoolError::NoMetadata,
    })
    .await
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn hybrid_production_doesnt_produce_empty_blocks_when_txpool_is_empty() {
    // verify the PoA service doesn't alter the hybrid block timing when
    // receiving txpool events if txpool is actually empty
    let mut rng = StdRng::seed_from_u64(2322);
    let secret_key = SecretKey::random(&mut rng);

    const TX_IDLE_TIME_MS: u64 = 50u64;

    let (txpool_tx, _txpool_broadcast) = broadcast::channel(10);
    let (import_block_events_tx, mut import_block_receiver_tx) = broadcast::channel(1);
    tokio::spawn(async move {
        let _ = import_block_receiver_tx.recv().await;
    });

    let mut block_producer = MockBlockProducer::default();

    block_producer
        .expect_produce_and_execute_block()
        .returning(|_, _| panic!("Block production should not be called"));

    let mut db = MockDatabase::default();
    db.expect_block_height()
        .returning(|| Ok(BlockHeight::from(1u32)));

    let mut txpool = MockTxPool::no_tx_updates();
    txpool.expect_total_consumable_gas().returning(|| 0);
    txpool.expect_pending_number().returning(|| 0);

    let task = PoA {
        block_gas_limit: 1000000,
        signing_key: Some(Secret::new(secret_key.into())),
        db,
        block_producer,
        txpool,
        import_block_events_tx,
        last_block_created: Instant::now(),
        trigger: Trigger::Hybrid {
            min_block_time: Duration::from_millis(100),
            max_tx_idle_time: Duration::from_millis(TX_IDLE_TIME_MS),
            max_block_time: Duration::from_millis(1000),
        },
        timer: DeadlineClock::new(),
    };

    let service = Service::new(task);
    service.start().unwrap();

    // simulate some txpool events to see if any block production is erroneously triggered
    txpool_tx.send(TxStatus::Submitted).unwrap();
    txpool_tx.send(TxStatus::Completed).unwrap();
    txpool_tx
        .send(TxStatus::SqueezedOut {
            reason: TxPoolError::NoMetadata,
        })
        .unwrap();

    // wait max_tx_idle_time - causes block production to occur if
    // pending txs > 0 is not checked.
    time::sleep(Duration::from_millis(TX_IDLE_TIME_MS)).await;

    service.stop_and_await().await.unwrap();
    assert!(service.state().stopped());
}

#[tokio::test(start_paused = true)] // Run with time paused, start/stop must still work
async fn clean_startup_shutdown_each_trigger() -> anyhow::Result<()> {
    for trigger in [
        Trigger::Never,
        Trigger::Instant,
        Trigger::Interval {
            block_time: Duration::new(1, 0),
        },
        Trigger::Hybrid {
            min_block_time: Duration::new(1, 0),
            max_tx_idle_time: Duration::new(1, 0),
            max_block_time: Duration::new(1, 0),
        },
    ] {
        let db = MockDatabase::new();
        let (txpool, _broadcast_rx) = MockTxPool::spawn();
        let config = Config {
            trigger,
            block_gas_limit: 100_000,
            signing_key: Some(test_signing_key()),
            metrics: false,
        };

        let service = new_service(
            config,
            txpool.sender(),
            txpool.import_block_tx.clone(),
            MockBlockProducer::new(txpool.sender(), db.clone()),
            db,
        );
        service.start()?;

        service.stop_and_await().await?;
    }

    Ok(())
}

struct CoinInfo {
    index: u8,
    id: Bytes32,
    secret_key: SecretKey,
}

impl CoinInfo {
    pub fn utxo_id(&self) -> UtxoId {
        UtxoId::new(self.id, self.index)
    }
}

fn _make_tx(coin: &CoinInfo, gas_price: u64, gas_limit: u64) -> PoolTransaction {
    TransactionBuilder::script(vec![Opcode::RET(REG_ZERO)].into_iter().collect(), vec![])
        .gas_price(gas_price)
        .gas_limit(gas_limit)
        .add_unsigned_coin_input(
            coin.secret_key,
            coin.utxo_id(),
            1_000_000_000,
            AssetId::zeroed(),
            Default::default(),
            0,
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: AssetId::zeroed(),
        })
        .finalize_checked_basic(Default::default(), &Default::default())
        .into()
}

fn make_tx_trigger(rng: &mut StdRng) -> PoolTransaction {
    _make_tx(
        &CoinInfo {
            index: 0,
            id: rng.gen(),
            secret_key: SecretKey::random(rng),
        },
        1,
        10_000,
    )
}

#[tokio::test(start_paused = true)]
async fn never_trigger_never_produces_blocks() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Never,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db,
    );
    service.start()?;

    // Submit some txs
    let mut rng = StdRng::seed_from_u64(1234u64);
    for _ in 0..10 {
        txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
    }

    // Make sure enough time passes for the block to be produced
    time::sleep(Duration::new(10, 0)).await;

    // Make sure no blocks are produced
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn instant_trigger_produces_block_instantly() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Instant,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db.clone(),
    );
    service.start()?;

    // Submit tx
    let mut rng = StdRng::seed_from_u64(1234u64);
    txpool.add_tx(Arc::new(make_tx(&mut rng))).await;

    // Make sure it's produced
    assert_eq!(txpool.wait_block_produced().await, 1);

    // TODO: Check block_import is triggered

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_produces_blocks_periodically() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db,
    );
    service.start()?;

    // Make sure no blocks are produced yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a single block is produced, and a bit more
    time::sleep(Duration::new(3, 0)).await;

    // Make sure the empty block is actually produced
    assert_eq!(txpool.check_block_produced(), Ok(0));

    // Submit tx
    let mut rng = StdRng::seed_from_u64(1234u64);
    txpool.add_tx(Arc::new(make_tx(&mut rng))).await;

    // Make sure no blocks are produced before next interval
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure it's produced
    assert_eq!(txpool.check_block_produced(), Ok(1));

    // Submit two tx
    let mut rng = StdRng::seed_from_u64(1234u64);
    for _ in 0..2 {
        txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
    }

    time::sleep(Duration::from_millis(1)).await;

    // Make sure blocks are not produced before the block time is used
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert_eq!(txpool.check_block_produced(), Ok(2));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert_eq!(txpool.check_block_produced(), Ok(0));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_doesnt_react_to_full_txpool() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db,
    );
    service.start()?;

    // Fill txpool completely
    let mut rng = StdRng::seed_from_u64(1234u64);
    for _ in 0..1_000 {
        txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
        tokio::spawn(async {}).await.unwrap(); // Process messages so the channel doesn't lag
    }

    // Make sure blocks are not produced before the block time has elapsed
    time::sleep(Duration::new(1, 0)).await;
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure only one block per round is produced
    for _ in 0..5 {
        time::sleep(Duration::new(2, 0)).await;
        assert!(txpool.check_block_produced().is_ok());
        assert_eq!(
            txpool.check_block_produced(),
            Err(mpsc::error::TryRecvError::Empty)
        );
    }

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(2, 0),
            max_tx_idle_time: Duration::new(3, 0),
            max_block_time: Duration::new(10, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db,
    );
    service.start()?;

    // Make sure no blocks are produced yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure no blocks are produced when txpool is empty and max_block_time is not exceeded
    time::sleep(Duration::new(9, 0)).await;

    // Make sure the empty block is actually produced
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Submit tx
    let mut rng = StdRng::seed_from_u64(1234u64);
    txpool.add_tx(Arc::new(make_tx(&mut rng))).await;

    // Make sure no block is produced immediately, as none of the timers has expired yet
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Pass time until a single block is produced after idle time
    time::sleep(Duration::new(4, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(1));
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure the empty block is produced after max_block_time
    time::sleep(Duration::new(10, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(0));

    // Submit two tx
    let mut rng = StdRng::seed_from_u64(1234u64);
    for _ in 0..2 {
        txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
    }

    // Wait for both max_tx_idle_time and min_block_time to pass, and see that the block is produced
    time::sleep(Duration::new(4, 0)).await;
    assert_eq!(txpool.check_block_produced(), Ok(2));

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_reacts_correctly_to_full_txpool() -> anyhow::Result<()> {
    let db = MockDatabase::new();
    let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
    let producer = MockBlockProducer::new(txpool.sender(), db.clone());
    let config = Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(2, 0),
            max_tx_idle_time: Duration::new(3, 0),
            max_block_time: Duration::new(10, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    };

    let service = new_service(
        config,
        txpool.sender(),
        txpool.import_block_tx.clone(),
        producer,
        db,
    );
    service.start()?;

    // Fill txpool completely
    let mut rng = StdRng::seed_from_u64(1234u64);
    for _ in 0..100 {
        txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
        tokio::task::yield_now().await; // Process messages so the channel doesn't lag
    }

    // Make sure blocks are not produced before the min block time has elapsed
    time::sleep(Duration::new(1, 0)).await;
    assert_eq!(
        txpool.check_block_produced(),
        Err(mpsc::error::TryRecvError::Empty)
    );

    // Make sure only blocks are produced immediately after min_block_time, but no sooner
    for _ in 0..5 {
        time::sleep(Duration::new(2, 0)).await;
        tokio::task::yield_now().await;
        let result = txpool.check_block_produced();
        assert!(result.is_ok());
        assert_eq!(
            txpool.check_block_produced(),
            Err(mpsc::error::TryRecvError::Empty)
        );
    }

    // Stop
    service.stop_and_await().await?;

    Ok(())
}

fn test_signing_key() -> Secret<SecretKeyWrapper> {
    let mut rng = StdRng::seed_from_u64(0);
    let secret_key = SecretKey::random(&mut rng);
    Secret::new(secret_key.into())
}
