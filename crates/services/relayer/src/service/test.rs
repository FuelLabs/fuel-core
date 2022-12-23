use crate::test_helpers::middleware::MockMiddleware;
use futures::TryStreamExt;

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
        eth_local_finalized: Some(1),
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    let contracts = vec![Default::default()];
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        Arc::new(eth_node),
        Config::DEFAULT_LOG_PAGE_SIZE,
    )
    .map_ok(|(_, l)| l)
    .try_concat()
    .await
    .unwrap();
    assert_eq!(result, logs);
}

#[tokio::test]
async fn deploy_height_does_not_override() {
    let mut mock_db = crate::mock_db::MockDb::default();
    mock_db.set_finalized_da_height(50u64.into()).await.unwrap();
    let config = Config {
        da_deploy_height: 20u64.into(),
        da_finalization: 1u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    let (tx, _) = watch::channel(false);
    let mut relayer = Task::new(tx, eth_node, mock_db.clone(), config);
    relayer.set_deploy_height().await;

    assert_eq!(*mock_db.get_finalized_da_height().await.unwrap(), 50);
}

#[tokio::test]
async fn deploy_height_does_override() {
    let mut mock_db = crate::mock_db::MockDb::default();
    mock_db.set_finalized_da_height(50u64.into()).await.unwrap();
    let config = Config {
        da_deploy_height: 52u64.into(),
        da_finalization: 1u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    let (tx, _) = watch::channel(false);
    let mut relayer = Task::new(tx, eth_node, mock_db.clone(), config);
    relayer.set_deploy_height().await;

    assert_eq!(*mock_db.get_finalized_da_height().await.unwrap(), 52);
}
