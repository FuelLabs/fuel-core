#![cfg(feature = "test-helpers")]

use fuel_core_interfaces::relayer::RelayerDb;
use fuel_relayer_new::{
    mock_db::MockDb,
    test_helpers::middleware::MockMiddleware,
    Relayer,
};

#[tokio::test]
async fn can_set_da_height() {
    let mock_db = MockDb::default();
    let relayer = Relayer::new();
    let middleware = MockMiddleware::default();

    assert_eq!(mock_db.get_finalized_da_height().await, 20);
}
