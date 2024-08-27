#![allow(non_snake_case)]
use crate::test_helpers::middleware::MockMiddleware;

use futures::TryStreamExt;
use test_case::test_case;

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
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await
    .unwrap();
    assert_eq!(result, logs);
}

#[tokio::test]
async fn quorum_agrees_on_logs() {
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

    // Given
    let provider = Provider::new(
        QuorumProvider::builder()
            .add_provider(WeightedProvider::new(eth_node.clone()))
            .add_provider(WeightedProvider::new(eth_node))
            .quorum(Quorum::Majority)
            .build(),
    );
    let contracts = vec![Default::default()];

    // When
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &provider,
        DEFAULT_LOG_PAGE_SIZE,
    )
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await
    .unwrap();

    // Then
    assert_eq!(result, logs);
}

#[tokio::test]
async fn quorum__disagree_on_logs() {
    let eth_node_two_logs = MockMiddleware::default();
    let eth_node_one_log = MockMiddleware::default();
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
    eth_node_two_logs.update_data(|data| data.logs_batch = vec![logs.clone()]);
    eth_node_one_log.update_data(|data| data.logs_batch = vec![vec![logs[0].clone()]]);

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 5,
        eth_local_finalized: Some(1),
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Given
    let provider = Provider::new(
        QuorumProvider::builder()
            // 3 different providers with 3 different logs
            // 2 logs
            .add_provider(WeightedProvider::new(eth_node_two_logs))
            // 0 logs
            .add_provider(WeightedProvider::new(MockMiddleware::default()))
            // 1 log
            .add_provider(WeightedProvider::new(eth_node_one_log))
            .quorum(Quorum::Percentage(70))
            .build(),
    );
    let contracts = vec![Default::default()];

    // When
    let provider_error = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &provider,
        DEFAULT_LOG_PAGE_SIZE,
    )
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await;
    // Then

    match provider_error {
        Err(ProviderError::JsonRpcClientError(e)) => {
            assert_eq!(format!("{e}"), "No Quorum reached.");
        }
        _ => {
            panic!("Expected a JsonRpcClientError")
        }
    }
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
    let relayer = NotInitializedTask::new(eth_node, mock_db.clone(), config, false);
    let _ = relayer.into_task(&Default::default(), ()).await;

    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 50);
}

#[test_case(6, Some(6), Some(6); "if local is up to date with remote, then update sync")]
#[test_case(6, Some(100), Some(100); "if local is somehow ahead of remote, then update sync")]
#[test_case(6, Some(5), None; "if local is behind remote, then nothing sent to sync")]
#[test_case(6, None, None; "if local is not set, then nothing sent to sync")]
#[tokio::test]
async fn update_sync__changes_latest_eth_state(
    remote: u64,
    local: Option<u64>,
    expected: Option<u64>,
) {
    // given
    let mock_db = crate::mock_db::MockDb::default();
    let config = Config {
        da_deploy_height: 20u64.into(),
        ..Default::default()
    };
    let eth_node = MockMiddleware::default();
    let relayer = NotInitializedTask::new(eth_node, mock_db.clone(), config, false);
    let shared = relayer.shared_data();
    let task = relayer.into_task(&Default::default(), ()).await.unwrap();

    // when
    let eth_state = state::test_builder::TestDataSource {
        eth_remote_finalized: remote,
        eth_local_finalized: local,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();
    task.update_synced(&eth_state);

    // then
    let expected = expected.map(DaBlockHeight);
    let actual = *shared.synced.borrow().deref();
    assert_eq!(expected, actual);
}
