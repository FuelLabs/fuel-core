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

// Manual production is legal on interval nodes, which are commonly configured
// with a very large interval precisely to park auto-production (e.g. the
// o2-bench execution-time probe). The interval-mode executor-budget extension
// (FIX 3) must NOT apply to manual requests: adding the interval to the manual
// deadline hands the executor a deadline hours away, and the parallel scheduler
// (which closes the block only at its deadline) then hangs until the production
// timeout. The producer must receive the manual budget itself (~now + 1s).
#[tokio::test]
async fn manual_production_on_interval_node_keeps_the_manual_deadline() {
    use std::time::Instant as StdInstant;
    let huge_interval = Duration::from_secs(3_600);
    // Records (production_start, deadline_handed_to_producer) per block.
    let captured: Arc<StdMutex<Vec<(StdInstant, Instant)>>> =
        Arc::new(StdMutex::new(Vec::new()));

    let mut ctx_builder = TestContextBuilder::new();
    ctx_builder.with_config(Config {
        trigger: Trigger::Interval {
            block_time: huge_interval,
        },
        signer: SignMode::Key(test_signing_key()),
        metrics: false,
        ..Default::default()
    });
    let TxPoolContext { txpool, .. } = MockTransactionPool::new_with_txs(vec![]);
    ctx_builder.with_txpool(txpool);

    let mut importer = MockBlockImporter::default();
    importer.expect_commit_result().returning(|_| Ok(()));
    importer
        .expect_block_stream()
        .returning(|| Box::pin(tokio_stream::pending()));
    importer
        .expect_latest_block_height()
        .returning(|| Ok(Some(BlockHeight::from(0u32))));
    ctx_builder.with_importer(importer);

    let captured_in_producer = captured.clone();
    let mut producer = MockBlockProducer::default();
    producer
        .expect_produce_and_execute_block()
        .returning(move |_, time, _, deadline| {
            captured_in_producer
                .lock()
                .unwrap()
                .push((StdInstant::now(), deadline));
            let mut block = Block::default();
            block.header_mut().set_time(time);
            block.header_mut().recalculate_metadata();
            Ok(UncommittedResult::new(
                ExecutionResult {
                    block,
                    ..Default::default()
                },
                Default::default(),
            ))
        });
    ctx_builder.with_producer(producer);
    let ctx = ctx_builder.build().await;

    ctx.service
        .shared
        .manually_produce_block(
            Some(Tai64::now()),
            Mode::Blocks {
                number_of_blocks: 1,
            },
        )
        .await
        .unwrap();

    let recorded = captured.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1, "the producer was invoked once");
    let (start, deadline) = recorded[0];
    let budget = deadline.into_std().saturating_duration_since(start);
    // The manual budget is `now + 1s`; allow generous slack but require it to be
    // orders of magnitude below the interval (the bug handed out ~1h + 1s).
    assert!(
        budget <= Duration::from_secs(30),
        "manual production on an interval node must keep the manual deadline \
         (~1s), got a budget of {budget:?} — the interval leaked into it",
    );

    assert_eq!(ctx.stop().await, State::Stopped);
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
