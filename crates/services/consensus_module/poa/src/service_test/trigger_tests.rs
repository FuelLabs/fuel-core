use mockall::Sequence;
use tokio::{
    sync::Notify,
    time::Instant,
};

use super::*;

#[tokio::test(start_paused = true)] // Run with time paused, start/stop must still work
async fn clean_startup_shutdown_each_trigger() -> anyhow::Result<()> {
    for trigger in [
        Trigger::Never,
        Trigger::Instant,
        Trigger::Interval {
            block_time: Duration::new(1, 0),
        },
    ] {
        let mut ctx_builder = TestContextBuilder::new();
        ctx_builder.with_config(Config {
            trigger,
            signer: SignMode::Key(test_signing_key()),
            metrics: false,
            ..Default::default()
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
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });

    // initialize txpool with some txs
    let txs = (0..TX_COUNT).map(|_| make_tx(&mut rng)).collect::<Vec<_>>();
    let TxPoolContext {
        txpool,
        status_sender,
        ..
    } = MockTransactionPool::new_with_txs(txs.clone());
    ctx_builder.with_txpool(txpool);

    let mut importer = MockBlockImporter::default();
    importer
        .expect_commit_result()
        .returning(|_| panic!("Should not commit result"));
    importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));
    ctx_builder.with_importer(importer);
    let ctx = ctx_builder.build();
    for tx in txs {
        status_sender.send_replace(Some(tx.id(&ChainId::default())));
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
    status_sender: Arc<watch::Sender<Option<TxId>>>,
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
        importer
            .expect_block_stream()
            .returning(|| Box::pin(tokio_stream::pending()));
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
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });
    ctx.status_sender.send_replace(Some(TxId::zeroed()));

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
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });
    ctx.status_sender.send_replace(Some(TxId::zeroed()));

    // Make sure no blocks are produced yet
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a single block is produced, and a bit more
    time::sleep(Duration::new(3, 0)).await;

    // Make sure the empty block is actually produced
    assert!(ctx.block_import.try_recv().is_ok());
    // Emulate tx status update to trigger the execution
    ctx.status_sender.send_replace(Some(TxId::zeroed()));

    // Make sure no blocks are produced before next interval
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure it's produced
    assert!(ctx.block_import.try_recv().is_ok());

    // Emulate tx status update to trigger the execution
    ctx.status_sender.send_replace(Some(TxId::zeroed()));

    time::sleep(Duration::from_millis(1)).await;

    // Make sure blocks are not produced before the block time is used
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Pass time until a the next block is produced
    time::sleep(Duration::new(2, 0)).await;

    // Make sure only one block is produced
    assert!(ctx.block_import.try_recv().is_ok());
    assert!(matches!(
        ctx.block_import.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}


#[tokio::test(start_paused = true)]
async fn service__if_commit_result_fails_then_retry_commit_result_after_one_second() -> anyhow::Result<()> {
    // given
    let config = Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    };
    let block_production_waitpoint = Arc::new(Notify::new());
    let block_production_waitpoint_trigger = block_production_waitpoint.clone();
    
    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(config);
    let mut mock_tx_pool = MockTransactionPool::no_tx_updates();
    mock_tx_pool.expect_remove_txs().returning(|_| vec![]);
    ctx_builder.with_txpool(mock_tx_pool);

    let mut importer = MockBlockImporter::default();
    let mut seq = Sequence::new();
    // First attempt fails
    importer
        .expect_commit_result()
        .times(1)
        .in_sequence(&mut seq)
        .returning(move |_| Err(anyhow::anyhow!("Error in production")));
    // Second attempt should be triggered after 1 second and success
    importer
        .expect_commit_result()
        .times(1)
        .in_sequence(&mut seq)
        .returning(move |_| {
            block_production_waitpoint_trigger.notify_waiters();
            Ok(())
        });
    importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));
    ctx_builder.with_importer(importer);
    let test_ctx = ctx_builder.build();

    let before_retry = Instant::now();

    // when
    block_production_waitpoint.notified().await;
    
    // then
    assert!(before_retry.elapsed() >= Duration::from_secs(1));
    
    test_ctx.service.stop_and_await().await?;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn interval_trigger_doesnt_react_to_full_txpool() -> anyhow::Result<()> {
    let mut ctx = DefaultContext::new(Config {
        trigger: Trigger::Interval {
            block_time: Duration::new(2, 0),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });

    // Brackets to release the lock.
    {
        let mut guard = ctx.txs.lock().unwrap();
        // Fill txpool completely and notify about new transaction.
        for _ in 0..1_000 {
            guard.push(make_tx(&mut ctx.rng));
        }
        ctx.status_sender.send_replace(Some(TxId::zeroed()));
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
        assert!(ctx.block_import.try_recv().is_ok());
        assert!(matches!(
            ctx.block_import.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    // Stop
    ctx.test_ctx.service.stop_and_await().await?;

    Ok(())
}
