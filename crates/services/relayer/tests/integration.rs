#![cfg(feature = "test-helpers")]

use ethers_core::types::U256;
use fuel_core_relayer::{
    bridge::MessageSentFilter,
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
async fn can_get_messages() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();

    let config = Config::default();
    let contract_address = config.eth_v2_listening_contracts[0];
    let message = |nonce: u64, block_number: u64| {
        let message = MessageSentFilter {
            nonce: U256::from_dec_str(nonce.to_string().as_str())
                .expect("Should convert to U256"),
            ..Default::default()
        };
        let mut log = message.into_log();
        log.address = contract_address;
        log.block_number = Some(block_number.into());
        log
    };

    let logs = vec![message(1, 3), message(2, 5)];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(100.into()));
    let relayer = new_service_test(eth_node, mock_db.clone(), config);
    relayer.start_and_await().await.unwrap();

    relayer.shared.await_synced().await.unwrap();

    for msg in expected_messages {
        assert_eq!(mock_db.get_message(msg.id()).unwrap(), msg);
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
