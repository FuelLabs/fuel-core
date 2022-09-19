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

    let remote = state::EthRemote::current(20).finalization_period(15);
    let eth_state = state::EthLocal::finalized(2).with_remote(remote);

    let contracts = vec![Default::default()];
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &eth_node,
    )
    .await
    .unwrap();
    assert_eq!(result, logs);
}
