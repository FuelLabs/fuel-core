#![cfg(feature = "test-helpers")]

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
        middleware::{
            MockMiddleware,
            TriggerType,
        },
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
    let mock_db = MockDb::default();
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
    let mock_db = MockDb::default();
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
#[allow(non_snake_case)]
async fn relayer__downloads_message_logs_to_events_table() {
    let message = |nonce: u64, block_number: u64| {
        let message = MessageSentFilter {
            nonce: U256::from_dec_str(nonce.to_string().as_str())
                .expect("Should convert to U256"),
            ..Default::default()
        };
        let mut log = message.into_log();
        log.block_number = Some(block_number.into());
        log
    };

    // setup mock data
    let logs = vec![message(1, 3), message(2, 5)];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    let mut ctx = TestContext::new();
    // given logs
    ctx.given_logs(logs);
    // when the relayer runs
    let mock_db = ctx.when_relayer_syncs().await;
    // expect several messages in the database
    for msg in expected_messages {
        assert_eq!(mock_db.get_message(msg.id()).unwrap(), msg);
    }
}

#[tokio::test(start_paused = true)]
#[allow(non_snake_case)]
async fn relayer__downloads_transaction_logs_to_events_table() {
    let transaction = |max_gas: u64, block_number: u64| {
        let transaction = TransactionFilter {
            max_gas,
            ..Default::default()
        };
        let mut log = transaction.into_log();
        log.block_number = Some(block_number.into());
        log
    };

    // setup mock data
    let logs = vec![transaction(2, 1), transaction(3, 2)];
    let expected_transactions: Vec<_> = logs.iter().map(|l| l.to_tx()).collect();
    let mut ctx = TestContext::new();
    // given logs
    ctx.given_logs(logs);
    // when the relayer runs
    let mock_db = ctx.when_relayer_syncs().await;
    // expect several transaction events in the database
    for tx in expected_transactions {
        assert_eq!(mock_db.get_transaction(&tx.id()).unwrap(), tx);
    }
}

#[tokio::test(start_paused = true)]
async fn deploy_height_is_set() {
    let mock_db = MockDb::default();
    let config = Config {
        da_deploy_height: 50u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    eth_node.update_data(|data| data.best_block.number = Some(54.into()));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    eth_node.set_before_event(move |_, evt| {
        if let TriggerType::GetLogs(evt) = evt {
            assert!(
                matches!(
                    evt,
                    ethers_core::types::Filter {
                        block_option: ethers_core::types::FilterBlockOption::Range {
                            from_block: Some(ethers_core::types::BlockNumber::Number(f)),
                            to_block: Some(ethers_core::types::BlockNumber::Number(t))
                        },
                        ..
                    } if f.as_u64() == 51 && t.as_u64() == 54
                ),
                "{evt:?}"
            );
            if let Some(tx) = tx.take() {
                tx.send(()).unwrap();
            }
        }
    });
    let relayer = new_service_test(eth_node, mock_db.clone(), config);
    relayer.start_and_await().await.unwrap();
    relayer.shared.await_synced().await.unwrap();
    rx.await.unwrap();
    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 54);
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
        let contract_address = self.config.eth_v2_listening_contracts[0];
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
