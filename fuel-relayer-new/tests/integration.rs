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
    let relayer = RelayerHandle::start();
    let middleware = MockMiddleware::default();

    relayer.await_synced().await;

    assert_eq!(mock_db.get_finalized_da_height().await, 20);
}
