use crate::test_helpers::middleware::MockMiddleware;

use super::*;

#[tokio::test]
async fn can_download_logs() {
    let eth_node = MockMiddleware::default();
    let logs = vec![
        Log {
            address: Default::default(),
            block_number: Some(3.into()),
            ..Default::default()
        },
        Log {
            address: Default::default(),
            block_number: Some(5.into()),
            ..Default::default()
        },
    ];
    eth_node.data.lock().await.logs_batch = vec![logs.clone()];

    let current_local_block_height = 0;
    let latest_finalized_block = 5;
    let contracts = vec![Default::default()];
    let result = download_logs(
        current_local_block_height,
        latest_finalized_block,
        contracts,
        &eth_node,
    )
    .await
    .unwrap();
    assert_eq!(result, logs);
}
