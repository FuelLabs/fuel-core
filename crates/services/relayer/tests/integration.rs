#![cfg(feature = "test-helpers")]
#![allow(non_snake_case)]

use ethers_core::types::{
    Log,
    U256,
};
use fuel_core_relayer::{
    bridge::{
        MessageSentFilter,
        TransactionFilter,
    },
    mock_db::MockDb,
    new_service_test,
    ports::RelayerDb,
    test_helpers::{
        middleware::MockMiddleware,
        EvtToLog,
        LogTestHelper,
    },
    Config,
};
use fuel_core_services::Service;

fuel_core_trace::enable_tracing!();

#[tokio::test(start_paused = true)]
async fn can_set_da_height() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(100.into()));
    let relayer = new_service_test(eth_node, mock_db.clone(), Default::default());
    relayer.start_and_await().await.unwrap();

    relayer.shared.await_synced().await.unwrap();

    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 100);
}

#[tokio::test(start_paused = true)]
async fn stop_service_at_the_begin() {
    // The test verifies that if the service is stopped at the beginning, it will sync nothing.
    // It is possible to test because we simulate delay from the Ethereum node by the
    // `tokio::task::yield_now().await` in each method of the `MockMiddleware`.
    let mut mock_db = MockDb::default();
    mock_db
        .set_finalized_da_height_to_at_least(&0u64.into())
        .unwrap();
    let eth_node = MockMiddleware::default();
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(100.into()));
    let relayer = new_service_test(eth_node, mock_db.clone(), Default::default());
    relayer.start_and_await().await.unwrap();
    relayer.stop();

    assert!(relayer.shared.await_synced().await.is_err());
    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 0);
}

#[tokio::test(start_paused = true)]
async fn stop_service_at_the_middle() {
    // The test verifies that if the service is stopped in the middle of the synchronization,
    // it will stop immediately.
    // It is possible to test because we simulate delay from the Ethereum node by the
    // `tokio::task::yield_now().await` in each method of the `MockMiddleware`.
    let mut mock_db = MockDb::default();
    mock_db
        .set_finalized_da_height_to_at_least(&0u64.into())
        .unwrap();
    let eth_node = MockMiddleware::default();
    eth_node.update_data(|data| data.best_block.number = Some(100.into()));
    let config = Config {
        log_page_size: 5,
        ..Default::default()
    };
    let relayer = new_service_test(eth_node, mock_db.clone(), config);
    relayer.start_and_await().await.unwrap();

    // Skip the initial requests to start the synchronization.
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;

    // Allow several sync iterations.
    const NUMBER_OF_SYNC_ITERATIONS: u64 = 7;
    for _ in 0..NUMBER_OF_SYNC_ITERATIONS {
        // Each iterations syncs 5 blocks.
        tokio::task::yield_now().await;
    }
    relayer.stop();

    assert!(relayer.shared.await_synced().await.is_err());
    // During each iteration we sync 5 blocks, so the final 5 * `NUMBER_OF_SYNC_ITERATIONS`
    assert_eq!(
        *mock_db.get_finalized_da_height().unwrap(),
        NUMBER_OF_SYNC_ITERATIONS * 5
    );
}

#[tokio::test(start_paused = true)]
async fn relayer__downloads_message_logs_to_events_table() {
    // setup mock data
    let block_one = 1;
    let block_two = 2;
    let block_three_msg = message(3, block_one, 0);
    let block_five_msg = message(2, block_two, 0);
    let logs = vec![block_three_msg, block_five_msg];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    let mut ctx = TestContext::new();
    // given logs
    ctx.given_logs(logs);
    // when the relayer runs
    let mock_db = ctx.when_relayer_syncs().await;
    // then
    let block_one_actual = mock_db
        .get_messages_for_block(block_one.into())
        .pop()
        .unwrap();
    assert_eq!(block_one_actual, expected_messages[0]);
    let block_two_actual = mock_db
        .get_messages_for_block(block_two.into())
        .pop()
        .unwrap();
    assert_eq!(block_two_actual, expected_messages[1]);
}

#[tokio::test(start_paused = true)]
async fn relayer__downloads_transaction_logs_to_events_table() {
    // setup mock data
    let block_one_tx = transaction(2, 1, 0);
    let block_two_tx = transaction(3, 2, 0);
    let logs = vec![block_one_tx, block_two_tx];
    let expected_transactions: Vec<_> = logs.iter().map(|l| l.to_tx()).collect();
    let mut ctx = TestContext::new();
    // given logs
    ctx.given_logs(logs);
    // when the relayer runs
    let mock_db = ctx.when_relayer_syncs().await;
    // then
    let block_one_actual = mock_db
        .get_transactions_for_block(1u64.into())
        .pop()
        .unwrap();
    assert_eq!(block_one_actual, expected_transactions[0]);
    let block_two_actual = mock_db
        .get_transactions_for_block(2u64.into())
        .pop()
        .unwrap();
    assert_eq!(block_two_actual, expected_transactions[1]);
}

#[tokio::test(start_paused = true)]
async fn relayer__downloaded_transaction_logs_will_always_be_stored_in_block_index_order()
{
    let blocks = vec![1u64, 2, 3, 4];
    let ordered_logs: Vec<Log> = vec![
        // one
        transaction(2, 1, 0),
        transaction(4, 1, 1),
        transaction(8, 1, 2),
        // two
        transaction(3, 2, 0),
        transaction(7, 2, 1),
        // three
        transaction(4, 3, 0),
        transaction(9, 3, 1),
        transaction(2, 3, 2),
        // four
        transaction(3, 4, 0),
        transaction(7, 4, 1),
        transaction(5, 4, 2),
    ];
    let expected_transactions: Vec<_> = ordered_logs.iter().map(|l| l.to_tx()).collect();
    let mut ctx = TestContext::new();

    // given
    let shuffled_logs = ordered_logs.iter().cloned().rev().collect();
    ctx.given_logs(shuffled_logs);
    // when
    let mock_db = ctx.when_relayer_syncs().await;
    // then
    let mut actual = Vec::new();
    for block in blocks {
        actual.extend(mock_db.get_transactions_for_block(block.into()));
    }
    assert_eq!(actual, expected_transactions);
}

#[tokio::test(start_paused = true)]
async fn relayer__downloaded_message_logs_will_always_be_stored_in_block_index_order() {
    let blocks = vec![1u64, 2, 3, 4];
    let ordered_logs: Vec<Log> = vec![
        // one
        message(1, 1, 0),
        message(2, 1, 1),
        message(3, 1, 2),
        // two
        message(4, 2, 0),
        message(5, 2, 1),
        // three
        message(6, 3, 0),
        message(7, 3, 1),
        message(8, 3, 2),
        // four
        message(9, 4, 0),
        message(10, 4, 1),
        message(11, 4, 2),
    ];
    let expected_messages: Vec<_> = ordered_logs.iter().map(|l| l.to_msg()).collect();
    let mut ctx = TestContext::new();

    // given
    let shuffled_logs = ordered_logs.iter().cloned().rev().collect();
    ctx.given_logs(shuffled_logs);
    // when
    let mock_db = ctx.when_relayer_syncs().await;
    // then
    let mut actual = Vec::new();
    for block in blocks {
        let messages = mock_db.get_messages_for_block(block.into());
        actual.extend(messages);
    }
    assert_eq!(actual, expected_messages);
}

#[tokio::test(start_paused = true)]
async fn relayer__if_a_log_does_not_include_index_then_event_not_included_and_errors() {
    let mut ctx = TestContext::new();
    // given
    let block_height = 1;
    let mut log = message(3, block_height, 2);
    log.log_index = None;
    ctx.given_logs(vec![log]);
    // when
    let relayer = new_service_test(ctx.eth_node, ctx.mock_db.clone(), ctx.config);
    relayer.start_and_await().await.unwrap();
    let res = relayer.shared.await_synced().await;
    // then
    assert!(res.is_err());
    assert!(ctx
        .mock_db
        .get_messages_for_block(block_height.into())
        .is_empty());
}

fn message(nonce: u64, block_number: u64, block_index: u64) -> Log {
    let message = MessageSentFilter {
        nonce: U256::from_dec_str(nonce.to_string().as_str())
            .expect("Should convert to U256"),
        ..Default::default()
    };
    let mut log = message.into_log();
    log.block_number = Some(block_number.into());
    log.log_index = Some(block_index.into());
    log
}

fn transaction(max_gas: u64, block_number: u64, block_index: u64) -> Log {
    let transaction = TransactionFilter {
        max_gas,
        ..Default::default()
    };
    let mut log = transaction.into_log();
    log.block_number = Some(block_number.into());
    log.log_index = Some(block_index.into());
    log
}

struct TestContext {
    mock_db: MockDb,
    eth_node: MockMiddleware,
    config: Config,
}

impl TestContext {
    fn new() -> Self {
        Self {
            mock_db: MockDb::default(),
            eth_node: MockMiddleware::default(),
            config: Config::default(),
        }
    }

    fn given_logs(&mut self, mut logs: Vec<Log>) {
        let contract_address = fuel_core_relayer::test_helpers::convert_to_address(
            self.config.eth_v2_listening_contracts[0].as_slice(),
        );
        for log in &mut logs {
            log.address = contract_address;
        }

        self.eth_node
            .update_data(|data| data.logs_batch = vec![logs.clone()]);
        // Setup the eth node with a block high enough that there
        // will be some finalized blocks.
        self.eth_node
            .update_data(|data| data.best_block.number = Some(100.into()));
    }

    async fn when_relayer_syncs(self) -> MockDb {
        let relayer = new_service_test(self.eth_node, self.mock_db.clone(), self.config);
        relayer.start_and_await().await.unwrap();

        relayer.shared.await_synced().await.unwrap();

        self.mock_db
    }
}
