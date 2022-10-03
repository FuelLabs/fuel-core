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
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_current: 20,
        eth_remote_finalization_period: 15,
        eth_local_finalized: 2,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    let contracts = vec![Default::default()];
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        Arc::new(eth_node),
        Config::DEFAULT_LOG_PAGE_SIZE,
    )
    .await
    .unwrap();
    assert_eq!(result, logs);
}
