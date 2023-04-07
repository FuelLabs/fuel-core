use super::*;

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
            consensus_params: Default::default(),
        });
        let ctx = ctx_builder.build();

        assert_eq!(ctx.stop().await, State::Stopped);
    }

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn never_trigger_never_produces_blocks() {
    const TX_COUNT: usize = 10;
    let mut rng = StdRng::seed_from_u64(1234u64);
    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(Config {
        trigger: Trigger::Never,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // initialize txpool with some txs
    let TxPoolContext {
        txpool,
        status_sender,
        ..
    } = MockTransactionPool::new_with_txs(
        (0..TX_COUNT).map(|_| make_tx(&mut rng)).collect(),
    );
    ctx_builder.with_txpool(txpool);

    let mut importer = MockBlockImporter::default();
    importer
        .expect_commit_result()
        .returning(|_| panic!("Should not commit result"));
    ctx_builder.with_importer(importer);
    let ctx = ctx_builder.build();
    for _ in 0..TX_COUNT {
        status_sender.send_replace(Some(TxStatus::Submitted));
    }

    // Make sure enough time passes for the block to be produced
    time::sleep(Duration::new(10, 0)).await;

    // Stop
    assert_eq!(ctx.stop().await, State::Stopped);
}

struct DefaultContext {
    rng: StdRng,
    test_ctx: TestContext,
    block_import: broadcast::Receiver<SealedBlock>,
    status_sender: Arc<watch::Sender<Option<TxStatus>>>,
    txs: Arc<StdMutex<Vec<Script>>>,
}

impl DefaultContext {
    fn new(config: Config) -> Self {
        let mut rng = StdRng::seed_from_u64(1234u64);
        let mut ctx_builder = TestContextBuilder::new();
        ctx_builder.with_config(config);
        // initialize txpool with some txs
        let tx1 = make_tx(&mut rng);
        let TxPoolContext {
            txpool,
            status_sender,
            txs,
        } = MockTransactionPool::new_with_txs(vec![tx1]);
        ctx_builder.with_txpool(txpool);

        let (block_import_sender, block_import_receiver) = broadcast::channel(100);
        let mut importer = MockBlockImporter::default();
        importer.expect_commit_result().returning(move |result| {
            let (result, _) = result.into();
            let sealed_block = result.sealed_block;
            block_import_sender.send(sealed_block)?;
            Ok(())
        });
        ctx_builder.with_importer(importer);

        let test_ctx = ctx_builder.build();

        Self {
            rng,
            test_ctx,
            block_import: block_import_receiver,
            status_sender,
            txs,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn instant_trigger_produces_block_instantly() {
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Instant,
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    // Make sure it's produced
    assert!(ctx.block_import.recv().await.is_ok());

    // Stop
    assert_eq!(ctx.test_ctx.stop().await, State::Stopped);
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_produces_blocks_periodically() -> anyhow::Result<()> {
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    // Make sure no blocks are produced yet
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a single block is produced, and a bit more
    time::sleep(Duration::new(3, 0)).await;

    // Make sure the empty block is actually produced
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
    // Emulate tx status update to trigger the execution
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    // Make sure no blocks are produced before next interval
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure it's produced
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));

    // Emulate tx status update to trigger the execution
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    time::sleep(Duration::from_millis(1)).await;

    // Make sure blocks are not produced before the block time is used
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_doesnt_react_to_full_txpool() -> anyhow::Result<()> {
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // Brackets to release the lock.
    {
        let mut guard = ctx.txs.lock().unwrap();
        // Fill txpool completely and notify about new transaction.
        for _ in 0..1_000 {
            guard.push(make_tx(&mut ctx.rng));
        }
        ctx.status_sender.send_replace(Some(TxStatus::Submitted));
    }

    // Make sure blocks are not produced before the block time has elapsed
    time::sleep(Duration::new(1, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Make sure only one block per round is produced
    for _ in 0..5 {
        time::sleep(Duration::new(2, 0)).await;
        assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
        assert!(matches!(
            ctx.block_import.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly_max_block_time() -> anyhow::Result<()> {
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(2, 0),
            max_tx_idle_time: Duration::new(3, 0),
            max_block_time: Duration::new(10, 0),
        },
        block_gas_limit: 100_000,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // Make sure no blocks are produced yet
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Make sure no blocks are produced when txpool is empty and max_block_time is not exceeded
    time::sleep(Duration::new(9, 0)).await;

    // Make sure the empty block is actually produced
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly_max_block_time_not_overrides_max_tx_idle_time(
) -> anyhow::Result<()> {
    const MIN_BLOCK_TIME: u64 = 6;
    const MAX_TX_IDLE_TIME: u64 = 3;
    const MAX_BLOCK_TIME: u64 = 10;
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(MIN_BLOCK_TIME, 0),
            max_tx_idle_time: Duration::new(MAX_TX_IDLE_TIME, 0),
            max_block_time: Duration::new(MAX_BLOCK_TIME, 0),
        },
        // We want to test behaviour when the gas of all transactions < `block_gas_limit`
        block_gas_limit: Word::MAX,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // Make sure no blocks are produced when txpool is empty and `MAX_BLOCK_TIME` is not exceeded
    time::sleep(Duration::new(9, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Emulate tx status update to trigger the execution. It should produce the block after
    // `MAX_TX_IDLE_TIME`.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly_max_tx_idle_time() -> anyhow::Result<()>
{
    const MIN_BLOCK_TIME: u64 = 6;
    const MAX_TX_IDLE_TIME: u64 = 3;
    const MAX_BLOCK_TIME: u64 = 10;
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(MIN_BLOCK_TIME, 0),
            max_tx_idle_time: Duration::new(MAX_TX_IDLE_TIME, 0),
            max_block_time: Duration::new(MAX_BLOCK_TIME, 0),
        },
        // We want to test behaviour when the gas of all transactions < `block_gas_limit`
        block_gas_limit: Word::MAX,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Emulate tx status update to trigger the execution.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(1, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(1, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly_min_block_time_min_block_gas_limit(
) -> anyhow::Result<()> {
    const MIN_BLOCK_TIME: u64 = 4;
    const MAX_TX_IDLE_TIME: u64 = 9;
    const MAX_BLOCK_TIME: u64 = 10;
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(MIN_BLOCK_TIME, 0),
            max_tx_idle_time: Duration::new(MAX_TX_IDLE_TIME, 0),
            max_block_time: Duration::new(MAX_BLOCK_TIME, 0),
        },
        // We want to test behaviour when the gas of all transactions > `block_gas_limit`
        block_gas_limit: Word::MIN,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // Emulate tx status update to trigger the execution.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    time::sleep(Duration::new(MIN_BLOCK_TIME - 1, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(2, 0)).await;

    // Trigger the `produce_block` if `min_block_time` passed and block is full.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));
    tokio::task::yield_now().await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));

    for _ in 0..5 {
        time::sleep(Duration::new(MIN_BLOCK_TIME, 0)).await;
        assert!(matches!(
            ctx.block_import.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        tokio::task::yield_now().await;
        assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
        assert!(matches!(
            ctx.block_import.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}

// TODO: We found a bug https://github.com/FuelLabs/fuel-core/issues/866 in the hybrid logic,
//  we need to fix it=) Don't remove this test. Remove `ignore` when bug is resolved.
#[ignore]
#[tokio::test(start_paused = true)]
async fn hybrid_trigger_produces_blocks_correctly_min_block_time_max_block_gas_limit(
) -> anyhow::Result<()> {
    const MIN_BLOCK_TIME: u64 = 6;
    const MAX_TX_IDLE_TIME: u64 = 1;
    const MAX_BLOCK_TIME: u64 = 10;
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Hybrid {
            min_block_time: Duration::new(MIN_BLOCK_TIME, 0),
            max_tx_idle_time: Duration::new(MAX_TX_IDLE_TIME, 0),
            max_block_time: Duration::new(MAX_BLOCK_TIME, 0),
        },
        // We want to test behaviour when the gas of all transactions < `block_gas_limit`
        block_gas_limit: Word::MAX,
        signing_key: Some(test_signing_key()),
        metrics: false,
        consensus_params: Default::default(),
    });

    // Emulate tx status update to trigger the execution.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Emulate tx status update to trigger the execution.
    ctx.status_sender.send_replace(Some(TxStatus::Submitted));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(2, 0)).await;
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    time::sleep(Duration::new(3, 0)).await;
    assert!(matches!(ctx.block_import.try_recv(), Ok(_)));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}
