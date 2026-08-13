use crate::service::Mode;
use fuel_core_types::{
    blockchain::block::Block,
    tai64::Tai64,
};
use test_case::test_case;

use super::*;

#[test_case(Tai64::now(), 10, vec![Tai64::now(); 10], Trigger::Never, 0;
"can manually produce blocks even when trigger is Never")]
#[test_case(Tai64::now(), 10, vec![Tai64::now(); 10], Trigger::Instant, 0;
"can manually produce blocks when trigger is Instant")]
#[test_case(
    Tai64::now(), 3, vec![Tai64::now(), Tai64::now() + 10, Tai64::now() + 20],
    Trigger::Interval { block_time: Duration::from_secs(10) }, 0
; "can manually produce blocks with different times")]
#[test_case(Tai64::now() + 100, 10, vec![Tai64::now() + 100; 10], Trigger::Never, 0;
"can manually produce blocks starting in the future even when trigger is Never")]
#[test_case(Tai64::now() + 100, 10, vec![Tai64::now() + 100; 10], Trigger::Instant, 0;
"can manually produce blocks starting in the future when trigger is Instant")]
#[test_case(
    Tai64::now() + 100, 3, vec![Tai64::now() + 100, Tai64::now() + 110, Tai64::now() + 120],
    Trigger::Interval { block_time: Duration::from_secs(10) }, 0;
"can manually produce blocks starting in the future with different times")]
#[test_case(Tai64::now(), 10, vec![Tai64::now(); 10], Trigger::Never, 10;
"can manually produce blocks with txs even when trigger is Never")]
#[test_case(Tai64::now(), 10, vec![Tai64::now(); 10], Trigger::Instant, 10;
"can manually produce blocks with txs when trigger is Instant")]
#[test_case(
    Tai64::now(), 3, vec![Tai64::now(), Tai64::now() + 10, Tai64::now() + 20],
    Trigger::Interval { block_time: Duration::from_secs(10) }, 10
;
"can manually produce blocks with different times with txs")]
#[test_case(Tai64::now() + 100, 10, vec![Tai64::now() + 100; 10], Trigger::Never, 10;
"can manually produce blocks with txs starting in the future even when trigger is Never")]
#[test_case(Tai64::now() + 100, 10, vec![Tai64::now() + 100; 10], Trigger::Instant, 10;
"can manually produce blocks with txs starting in the future when trigger is Instant")]
#[test_case(
    Tai64::now() + 100, 3, vec![Tai64::now() + 100, Tai64::now() + 110, Tai64::now() + 120],
    Trigger::Interval { block_time: Duration::from_secs(10) }, 10;
"can manually produce blocks with txs starting in the future with different times")]
#[tokio::test]
async fn can_manually_produce_block(
    start_time: Tai64,
    number_of_blocks: u32,
    times: Vec<Tai64>,
    trigger: Trigger,
    num_txns: usize,
) {
    let mut rng = StdRng::seed_from_u64(1234u64);
    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(Config {
        trigger,
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });

    // initialize txpool with some txs
    let txs = (0..num_txns).map(|_| make_tx(&mut rng)).collect::<Vec<_>>();
    let TxPoolContext {
        txpool,
        new_txs_notifier,
        ..
    } = MockTransactionPool::new_with_txs(txs.clone());
    ctx_builder.with_txpool(txpool);

    let mut importer = MockBlockImporter::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel(times.len());
    importer.expect_commit_result().returning(move |r| {
        tx.try_send(r.into_result().sealed_block.entity.header().time())
            .unwrap();
        Ok(())
    });
    importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));
    importer
        .expect_latest_block_height()
        .returning(|| Ok(Some(BlockHeight::from(0u32))));
    importer
        .expect_latest_block_header()
        .returning(|| Ok(None));

    let mut producer = MockBlockProducer::default();
    producer
        .expect_produce_and_execute_block()
        .returning(|_, time, _, _| {
            let mut block = Block::default();
            block.header_mut().set_time(time);
            block.header_mut().recalculate_metadata();
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: Default::default(),
                    tx_status: Default::default(),
                    events: Default::default(),
                },
                Default::default(),
            ))
        });
    ctx_builder.with_importer(importer);
    ctx_builder.with_producer(producer);
    let ctx = ctx_builder.build().await;

    ctx.service
        .shared
        .manually_produce_block(Some(start_time), Mode::Blocks { number_of_blocks })
        .await
        .unwrap();
    new_txs_notifier.send_replace(());

    for t in times.into_iter() {
        let block_time = rx.recv().await.unwrap();
        assert_eq!(t, block_time);
    }

    // Stop
    assert_eq!(ctx.stop().await, State::Stopped);
}

/// Height-gap Synced can land before `block_stream` catches the DB tip. Manual /
/// Interval production must stamp from the DB parent header — not SyncTask's
/// older tip — or the next block gets a non-parent timestamp.
#[tokio::test]
async fn manual_produce__uses_db_tip_time_when_db_ahead_of_sync_task() {
    let sync_task_time = Tai64::now();
    let db_tip_time = sync_task_time + 10_000;
    let block_time = Duration::from_secs(10);

    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(Config {
        trigger: Trigger::Interval { block_time },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        min_connected_reserved_peers: 0,
        ..Default::default()
    });
    // SyncTask starts Synced at height 1 with an early timestamp.
    ctx_builder.with_start_time(sync_task_time.into());

    let mut importer = MockBlockImporter::default();
    importer.expect_commit_result().returning(|_| Ok(()));
    importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));
    importer
        .expect_latest_block_height()
        .returning(|| Ok(Some(BlockHeight::from(3u32))));
    // DB tip is ahead of SyncTask and carries the real parent time.
    importer.expect_latest_block_header().returning(move || {
        Ok(Some(BlockHeader::new_block(
            BlockHeight::from(3u32),
            db_tip_time,
        )))
    });

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut producer = MockBlockProducer::default();
    producer
        .expect_produce_and_execute_block()
        .returning(move |height, time, _, _| {
            tx.try_send((height, time)).unwrap();
            let mut block = Block::default();
            block.header_mut().set_block_height(height);
            block.header_mut().set_time(time);
            block.header_mut().recalculate_metadata();
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    skipped_transactions: Default::default(),
                    tx_status: Default::default(),
                    events: Default::default(),
                },
                Default::default(),
            ))
        });

    ctx_builder.with_importer(importer);
    ctx_builder.with_producer(producer);
    let ctx = ctx_builder.build().await;

    ctx.service
        .shared
        .manually_produce_block(None, Mode::Blocks { number_of_blocks: 1 })
        .await
        .expect("manual produce should succeed with DB tip refresh");

    let (height, time) = rx.recv().await.expect("produced block");
    assert_eq!(height, BlockHeight::from(4u32));
    assert_eq!(
        time,
        db_tip_time + block_time.as_secs(),
        "next block time must advance from DB tip, not SyncTask's older header"
    );
}

#[tokio::test]
async fn manually_block_production_fails_in_open_mode() {
    let mut rng = StdRng::seed_from_u64(1234u64);
    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(Config {
        trigger: Trigger::Open {
            period: Duration::from_secs(3),
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });

    // initialize txpool with some txs
    let tx = make_tx(&mut rng);
    let TxPoolContext { txpool, .. } =
        MockTransactionPool::new_with_txs(vec![tx.clone()]);
    ctx_builder.with_txpool(txpool);

    let ctx = ctx_builder.build().await;

    // When
    let result = ctx
        .service
        .shared
        .manually_produce_block(
            Some(Tai64::now()),
            Mode::Blocks {
                number_of_blocks: 1,
            },
        )
        .await;

    // Then
    let _ = result.expect_err("Expected error");
}
