use super::*;

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
        let mut ctx_builder = TestContextBuilder::new();
        ctx_builder.with_config(Config {
            trigger,
            block_gas_limit: 100_000,
            signing_key: Some(test_signing_key()),
            metrics: false,
        });
        let ctx = ctx_builder.build();

        ctx.stop().await;
    }

    Ok(())
}
#[tokio::test(start_paused = true)]
async fn never_trigger_never_produces_blocks() {
    let mut rng = StdRng::seed_from_u64(1234u64);
    let mut ctx_builder = TestContextBuilder::new();
    // initialize txpool with some txs

    let mock_txpool = MockTxPool::new_with_txs(
        (0..10)
            .map(|_| (make_tx(&mut rng), TxStatus::Submitted))
            .collect(),
    );
    ctx_builder.with_txpool(mock_txpool);
    ctx_builder.with_config(Config {
        trigger: Trigger::Never,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    });
    let ctx = ctx_builder.build();
    let mut block_import_rx = ctx.subscribe_import();

    // Make sure enough time passes for the block to be produced
    time::sleep(Duration::new(10, 0)).await;

    // Make sure no blocks are produced
    assert!(
        matches!(block_import_rx.try_recv(), Err(e) if e == broadcast::error::TryRecvError::Empty),
    );

    // Stop
    ctx.stop().await;
}

#[tokio::test(start_paused = true)]
async fn instant_trigger_produces_block_instantly() {
    let mut rng = StdRng::seed_from_u64(1234u64);
    let mut ctx_builder = TestContextBuilder::new();
    // initialize txpool with some txs
    let tx1 = make_tx(&mut rng);
    let mock_txpool = MockTxPool::new_with_txs(vec![(tx1, TxStatus::Submitted)]);
    ctx_builder.with_txpool(mock_txpool);
    // setup block producer mock
    let mut producer = MockBlockProducer::default();
    producer
        .expect_produce_and_execute_block()
        .returning(|_, _| {
            let mut db = MockDatabase::default();
            db.expect_as_mut().returning(move || {
                let mut tx_db = MockDatabase::default();
                tx_db.expect_seal_block().returning(|_, _| Ok(()));
                tx_db
            });
            db.expect_commit().returning(|| Ok(()));

            Ok(UncommittedResult::new(
                ExecutionResult {
                    block: Default::default(),
                    skipped_transactions: Default::default(),
                    tx_status: Default::default(),
                },
                StorageTransaction::new(db),
            ))
        });
    ctx_builder.with_producer(producer);
    ctx_builder.with_config(Config {
        trigger: Trigger::Instant,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
    });
    let ctx = ctx_builder.build();
    let mut block_import = ctx.block_import_tx.subscribe();

    // Make sure it's produced
    assert!(block_import.recv().await.is_ok());

    // Stop
    ctx.stop().await;
}
// #[tokio::test(start_paused = true)]
// async fn interval_trigger_produces_blocks_periodically() -> anyhow::Result<()> {
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     let mut ctx_builder = TestContextBuilder::new();
//     // initialize txpool with some txs
//     let tx1 = make_tx(&mut rng);
//     let mock_txpool = MockTxPool::new_with_txs(vec![(tx1, TxStatus::Submitted)]);
//     ctx_builder.with_txpool(mock_txpool);
//     // setup block producer mock
//     let mut producer = MockBlockProducer::default();
//     producer
//         .expect_produce_and_execute_block()
//         .returning(|_, _| {
//             let mut db = MockDatabase::default();
//             db.expect_as_mut().returning(move || {
//                 let mut tx_db = MockDatabase::default();
//                 tx_db.expect_seal_block().returning(|_, _| Ok(()));
//                 tx_db
//             });
//             db.expect_commit().returning(|| Ok(()));
//
//             Ok(UncommittedResult::new(
//                 ExecutionResult {
//                     block: Default::default(),
//                     skipped_transactions: Default::default(),
//                     tx_status: Default::default(),
//                 },
//                 StorageTransaction::new(db),
//             ))
//         });
//     ctx_builder.with_producer(producer);
//     ctx_builder.with_config(Config {
//         trigger: Trigger::Interval {
//             block_time: Duration::new(2, 0),
//         },
//         block_gas_limit: 100_000,
//         signing_key: Some(test_signing_key()),
//         metrics: false,
//     });
//     let ctx = ctx_builder.build();
//
//     // Make sure no blocks are produced yet
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Pass time until a single block is produced, and a bit more
//     time::sleep(Duration::new(3, 0)).await;
//
//     // Make sure the empty block is actually produced
//     assert_eq!(txpool.check_block_produced(), Ok(0));
//
//     // Submit tx
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//
//     // Make sure no blocks are produced before next interval
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Pass time until a the next block is produced
//     time::sleep(Duration::new(2, 0)).await;
//
//     // Make sure it's produced
//     assert_eq!(txpool.check_block_produced(), Ok(1));
//
//     // Submit two tx
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     for _ in 0..2 {
//         txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//     }
//
//     time::sleep(Duration::from_millis(1)).await;
//
//     // Make sure blocks are not produced before the block time is used
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Pass time until a the next block is produced
//     time::sleep(Duration::new(2, 0)).await;
//
//     // Make sure only one block is produced
//     assert_eq!(txpool.check_block_produced(), Ok(2));
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Pass time until a the next block is produced
//     time::sleep(Duration::new(2, 0)).await;
//
//     // Make sure only one block is produced
//     assert_eq!(txpool.check_block_produced(), Ok(0));
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Stop
//     service.stop_and_await().await?;
//
//     Ok(())
// }

// #[tokio::test(start_paused = true)]
// async fn interval_trigger_doesnt_react_to_full_txpool() -> anyhow::Result<()> {
//     let db = MockDatabase::new();
//     let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
//     let producer = MockBlockProducer::new(txpool.sender(), db.clone());
//     let config = Config {
//         trigger: Trigger::Interval {
//             block_time: Duration::new(2, 0),
//         },
//         block_gas_limit: 100_000,
//         signing_key: Some(test_signing_key()),
//         metrics: false,
//     };
//
//     let service = new_service(
//         config,
//         txpool.sender(),
//         txpool.import_block_tx.clone(),
//         producer,
//         db,
//     );
//     service.start()?;
//
//     // Fill txpool completely
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     for _ in 0..1_000 {
//         txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//         tokio::spawn(async {}).await.unwrap(); // Process messages so the channel doesn't lag
//     }
//
//     // Make sure blocks are not produced before the block time has elapsed
//     time::sleep(Duration::new(1, 0)).await;
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Make sure only one block per round is produced
//     for _ in 0..5 {
//         time::sleep(Duration::new(2, 0)).await;
//         assert!(txpool.check_block_produced().is_ok());
//         assert_eq!(
//             txpool.check_block_produced(),
//             Err(mpsc::error::TryRecvError::Empty)
//         );
//     }
//
//     // Stop
//     service.stop_and_await().await?;
//
//     Ok(())
// }
//
// #[tokio::test(start_paused = true)]
// async fn hybrid_trigger_produces_blocks_correctly() -> anyhow::Result<()> {
//     let db = MockDatabase::new();
//     let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
//     let producer = MockBlockProducer::new(txpool.sender(), db.clone());
//     let config = Config {
//         trigger: Trigger::Hybrid {
//             min_block_time: Duration::new(2, 0),
//             max_tx_idle_time: Duration::new(3, 0),
//             max_block_time: Duration::new(10, 0),
//         },
//         block_gas_limit: 100_000,
//         signing_key: Some(test_signing_key()),
//         metrics: false,
//     };
//
//     let service = new_service(
//         config,
//         txpool.sender(),
//         txpool.import_block_tx.clone(),
//         producer,
//         db,
//     );
//     service.start()?;
//
//     // Make sure no blocks are produced yet
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Make sure no blocks are produced when txpool is empty and max_block_time is not exceeded
//     time::sleep(Duration::new(9, 0)).await;
//
//     // Make sure the empty block is actually produced
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Submit tx
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//
//     // Make sure no block is produced immediately, as none of the timers has expired yet
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Pass time until a single block is produced after idle time
//     time::sleep(Duration::new(4, 0)).await;
//     assert_eq!(txpool.check_block_produced(), Ok(1));
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Make sure the empty block is produced after max_block_time
//     time::sleep(Duration::new(10, 0)).await;
//     assert_eq!(txpool.check_block_produced(), Ok(0));
//
//     // Submit two tx
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     for _ in 0..2 {
//         txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//     }
//
//     // Wait for both max_tx_idle_time and min_block_time to pass, and see that the block is produced
//     time::sleep(Duration::new(4, 0)).await;
//     assert_eq!(txpool.check_block_produced(), Ok(2));
//
//     // Stop
//     service.stop_and_await().await?;
//
//     Ok(())
// }
//
// #[tokio::test(start_paused = true)]
// async fn hybrid_trigger_reacts_correctly_to_full_txpool() -> anyhow::Result<()> {
//     let db = MockDatabase::new();
//     let (mut txpool, _broadcast_rx) = MockTxPool::spawn();
//     let producer = MockBlockProducer::new(txpool.sender(), db.clone());
//     let config = Config {
//         trigger: Trigger::Hybrid {
//             min_block_time: Duration::new(2, 0),
//             max_tx_idle_time: Duration::new(3, 0),
//             max_block_time: Duration::new(10, 0),
//         },
//         block_gas_limit: 100_000,
//         signing_key: Some(test_signing_key()),
//         metrics: false,
//     };
//
//     let service = new_service(
//         config,
//         txpool.sender(),
//         txpool.import_block_tx.clone(),
//         producer,
//         db,
//     );
//     service.start()?;
//
//     // Fill txpool completely
//     let mut rng = StdRng::seed_from_u64(1234u64);
//     for _ in 0..100 {
//         txpool.add_tx(Arc::new(make_tx(&mut rng))).await;
//         tokio::task::yield_now().await; // Process messages so the channel doesn't lag
//     }
//
//     // Make sure blocks are not produced before the min block time has elapsed
//     time::sleep(Duration::new(1, 0)).await;
//     assert_eq!(
//         txpool.check_block_produced(),
//         Err(mpsc::error::TryRecvError::Empty)
//     );
//
//     // Make sure only blocks are produced immediately after min_block_time, but no sooner
//     for _ in 0..5 {
//         time::sleep(Duration::new(2, 0)).await;
//         tokio::task::yield_now().await;
//         let result = txpool.check_block_produced();
//         assert!(result.is_ok());
//         assert_eq!(
//             txpool.check_block_produced(),
//             Err(mpsc::error::TryRecvError::Empty)
//         );
//     }
//
//     // Stop
//     service.stop_and_await().await?;
//
//     Ok(())
// }
