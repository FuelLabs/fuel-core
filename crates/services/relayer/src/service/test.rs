use crate::test_helpers::middleware::MockMiddleware;
use futures::TryStreamExt;

use super::*;

const DEFAULT_LOG_PAGE_SIZE: u64 = 5;

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
        eth_remote_finalized: 5,
        eth_local_finalized: Some(1),
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    let contracts = vec![Default::default()];
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &eth_node,
        DEFAULT_LOG_PAGE_SIZE,
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
    mock_db
        .set_finalized_da_height_to_at_least(&50u64.into())
        .unwrap();
    let config = Config {
        da_deploy_height: 20u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    let relayer = NotInitializedTask::new(eth_node, mock_db.clone(), config);
    let _ = relayer.into_task(&Default::default(), ()).await;

    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 50);
}

#[tokio::test]
async fn deploy_height_does_override() {
    let mut mock_db = crate::mock_db::MockDb::default();
    mock_db
        .set_finalized_da_height_to_at_least(&50u64.into())
        .unwrap();
    let config = Config {
        da_deploy_height: 52u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    let relayer = NotInitializedTask::new(eth_node, mock_db.clone(), config);
    let _ = relayer.into_task(&Default::default(), ()).await;

    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 52);
}
