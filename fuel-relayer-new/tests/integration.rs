#![cfg(feature = "test-helpers")]
use fuel_relayer_new::test_helpers::middleware::MockMiddleware;
use fuel_relayer_new::*;

#[tokio::test]
async fn can_set_da_height() {
    let relayer = Relayer::new();
    let middleware = MockMiddleware::default();
}
