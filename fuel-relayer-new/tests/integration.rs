#![cfg(feature = "test-helpers")]

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_relayer_new::{
    mock_db::MockDb,
    test_helpers::middleware::MockMiddleware,
    RelayerHandle,
};

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
