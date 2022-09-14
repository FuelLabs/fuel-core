#![cfg(feature = "test-helpers")]

use ethers_core::types::{
    Address,
    Log,
};
use fuel_core_interfaces::{
    common::prelude::Storage,
    relayer::RelayerDb,
};
use fuel_relayer_new::{
    mock_db::MockDb,
    test_helpers::{
        eth_log_message,
        middleware::MockMiddleware,
        LogTestHelper,
    },
    RelayerHandle,
};

fn make_log_msg(address: Address, block_number: u64, data: &[u8]) -> Log {
    eth_log_message(
        address,
        block_number,
        Default::default(),
        Default::default(),
        Default::default(),
        data.to_vec(),
    )
}

#[tokio::test]
async fn can_set_da_height() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.data.lock().await.best_block.number = Some(200.into());
    let relayer = RelayerHandle::start_test(
        eth_node,
        Box::new(mock_db.clone()),
        Default::default(),
    );

    relayer.await_synced().await.unwrap();

    assert_eq!(mock_db.get_finalized_da_height().await, 100);
}

#[tokio::test]
async fn can_get_messages() {
    let mock_db = MockDb::default();
    let eth_node = MockMiddleware::default();
    let logs = vec![
        make_log_msg(Default::default(), 3, &[]),
        make_log_msg(Default::default(), 5, &[]),
    ];
    let expected_messages: Vec<_> = logs.iter().map(|l| l.to_msg()).collect();
    eth_node.data.lock().await.logs_batch = vec![logs.clone()];
    // Setup the eth node with a block high enough that there
    // will be some finalized blocks.
    eth_node.data.lock().await.best_block.number = Some(200.into());
    let relayer = RelayerHandle::start_test(
        eth_node,
        Box::new(mock_db.clone()),
        Default::default(),
    );

    relayer.await_synced().await.unwrap();

    for msg in expected_messages {
        assert_eq!(mock_db.get(msg.id()).unwrap().unwrap().as_ref(), &*msg);
    }
}
