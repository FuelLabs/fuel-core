#![cfg(feature = "test-helpers")]

use fuel_core_relayer::{
    bridge::message::SentMessageFilter,
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

#[tokio::test(start_paused = true)]
async fn can_set_da_height() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = new_service_test(eth_node, mock_db.clone(), Default::default());
    relayer.start_and_await().await.unwrap();

    relayer.shared.await_synced().await.unwrap();

    assert_eq!(*mock_db.get_finalized_da_height().await.unwrap(), 100);
}

#[tokio::test(start_paused = true)]
async fn can_get_messages() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();

    let config = Config::default();
    let contract_address = config.eth_v2_listening_contracts[0];
    let message = |nonce, block_number: u64| {
        let message = SentMessageFilter {
            nonce,
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
    eth_node.update_data(|data| data.best_block.number = Some(200.into()));
    let relayer = new_service_test(eth_node, mock_db.clone(), config);
    relayer.start_and_await().await.unwrap();

    relayer.shared.await_synced().await.unwrap();

    for msg in expected_messages {
        assert_eq!(&mock_db.get_message(msg.id()).unwrap(), msg.message());
    }
}

#[tokio::test(start_paused = true)]
async fn deploy_height_is_set() {
    let mock_db = MockDb::default();
    let config = Config {
        da_deploy_height: 50u64.into(),
        da_finalization: 1u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    eth_node.update_data(|data| data.best_block.number = Some(54.into()));
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut tx = Some(tx);
    eth_node.set_before_event(move |_, evt| {
        if let fuel_core_relayer::test_helpers::middleware::TriggerType::GetLogs(evt) = evt {
            assert!(
                matches!(
                    evt,
                        ethers_core::types::Filter {
                            block_option: ethers_core::types::FilterBlockOption::Range {
                                from_block: Some(ethers_core::types::BlockNumber::Number(f)),
                                to_block: Some(ethers_core::types::BlockNumber::Number(t))
                            },
                            ..
                        }
                    if f.as_u64() == 51 && t.as_u64() == 53
                ),
                "{:?}",
                evt
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

    assert_eq!(*mock_db.get_finalized_da_height().await.unwrap(), 53);
}
