#![allow(non_snake_case)]

use crate::{
    Config,
    ports::RelayerDb,
    service::NotInitializedTask,
};
use alloy_primitives::LogData;
use fuel_core_provider::test_helpers::provider::MockProvider;
use fuel_core_services::RunnableService;
use futures::TryStreamExt;
use std::num::NonZeroU8;
use test_case::test_case;

use super::*;

const DEFAULT_LOG_PAGE_SIZE: u64 = 5;

use crate::{
    service::{
        download_logs,
        state,
    },
    test_helpers::page_sizer::IdentityPageSizer,
};
use alloy_provider::{
    ProviderBuilder,
    mock::Asserter,
    transport::{
        TransportError,
        TransportErrorKind,
    },
};
use alloy_rpc_types_eth::Log;

#[tokio::test]
async fn can_download_logs() {
    let asserter = Asserter::new();
    let logs = vec![
        Log {
            block_number: Some(3),
            ..Default::default()
        },
        Log {
            block_number: Some(5),
            ..Default::default()
        },
    ];
    asserter.push_success(&logs);
    let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone());

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 5,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    let contracts = vec![Default::default()];
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &provider,
        &mut IdentityPageSizer::new(DEFAULT_LOG_PAGE_SIZE),
    )
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await
    .unwrap();

    // Then
    assert_eq!(result, logs);
}

#[tokio::test]
async fn quorum_agrees_on_logs() {
    let asserter = Asserter::new();
    let logs = vec![
        Log {
            block_number: Some(3),
            ..Default::default()
        },
        Log {
            block_number: Some(5),
            ..Default::default()
        },
    ];
    asserter.push_success(&logs);

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 5,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Given
    let provider = QuorumProvider::builder()
        .with_mocked_transport(asserter.clone())
        .with_mocked_transport(asserter.clone())
        .with_quorum(Quorum::Majority)
        .build();
    let contracts = vec![Default::default()];

    // When
    let result = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &provider,
        &mut IdentityPageSizer::new(DEFAULT_LOG_PAGE_SIZE),
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
    let logs: Vec<Log<LogData>> = vec![
        Log {
            block_number: Some(3),
            ..Default::default()
        },
        Log {
            block_number: Some(5),
            ..Default::default()
        },
    ];

    let asserter_with_two_logs = Asserter::new();
    asserter_with_two_logs.push_success(&logs);

    let asserter_with_one_log = Asserter::new();
    asserter_with_one_log.push_success(&logs[0]);

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 5,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Given
    let provider = QuorumProvider::builder()
        // 3 different providers with 3 different logs
        // 2 logs
        .with_mocked_transport(asserter_with_two_logs)
        // 0 logs
        .with_mocked_transport(Asserter::new())
        // 1 log
        .with_mocked_transport(asserter_with_one_log)
        .with_quorum(Quorum::Percentage(NonZeroU8::new(70).unwrap()))
        .build();
    let contracts = vec![Default::default()];

    // When
    let provider_error = download_logs(
        &eth_state.needs_to_sync_eth().unwrap(),
        contracts,
        &provider,
        &mut IdentityPageSizer::new(DEFAULT_LOG_PAGE_SIZE),
    )
    .map_ok(|logs| logs.logs)
    .try_concat()
    .await;
    // Then

    match provider_error {
        Err(TransportError::Transport(TransportErrorKind::Custom(e))) => {
            assert!(
                e.to_string().starts_with(
                    "eth provider failed to get logs: Custom(NoQuorumReached"
                )
            );
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
    let eth_node = MockProvider::default();
    let relayer = NotInitializedTask::new(eth_node, mock_db.clone(), config, false);
    let _ = relayer.into_task(&Default::default(), ()).await;

    assert_eq!(*mock_db.get_finalized_da_height().unwrap(), 50);
}

const STARTING_HEIGHT: u64 = 2;

#[test_case(6, 6, SyncState::Synced(6u64.into()); "if local is up to date with remote, then fully synced state"
)]
#[test_case(6, 100, SyncState::Synced(100u64.into()); "if local is somehow ahead of remote, then fully synced state"
)]
#[test_case(6, 5, SyncState::PartiallySynced(5u64.into()); "if local is behind remote, then partially synced state"
)]
#[test_case(6, 0, SyncState::PartiallySynced(0u64.into()); "if local is set to starting height, then partially synced state"
)]
#[tokio::test]
async fn update_sync__changes_latest_eth_state(
    remote: u64,
    local: u64,
    expected: SyncState,
) {
    // given
    let mock_db = crate::mock_db::MockDb::default();
    let config = Config {
        da_deploy_height: STARTING_HEIGHT.into(),
        ..Default::default()
    };
    let eth_node = MockProvider::default();
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
    let actual = *shared.synced.borrow();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn relayer_grows_page_size_on_success() {
    let eth_node = MockProvider::default();
    let logs = vec![
        Log {
            block_number: Some(2),
            ..Default::default()
        },
        Log {
            block_number: Some(3),
            ..Default::default()
        },
    ];
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);

    let mock_db = crate::mock_db::MockDb::default();
    let config = Config {
        log_page_size: 10, // max page size
        ..Default::default()
    };

    let shutdown = StateWatcher::started();

    let mut relayer = NotInitializedTask::new(eth_node, mock_db, config, false)
        .into_task(&shutdown, ())
        .await
        .unwrap();

    relayer.page_sizer = AdaptivePageSizer::new(5, 10, 50, 10_000);
    relayer.page_sizer.successful_rpc_calls = 50;

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 10,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Act
    let result = relayer
        .download_logs(&eth_state.needs_to_sync_eth().unwrap())
        .await;

    // Assert
    assert!(result.is_ok());
    assert_eq!(relayer.page_sizer.page_size(), 6); // 5 * 125 / 100 = 6.25 → 6
}

#[tokio::test]
async fn relayer_respects_max_page_size_limit() {
    let eth_node = MockProvider::default();
    let logs = vec![Log {
        block_number: Some(2),
        ..Default::default()
    }];
    eth_node.update_data(|data| data.logs_batch = vec![logs.clone()]);

    let mock_db = crate::mock_db::MockDb::default();
    let config = Config {
        log_page_size: 10, // max page size
        ..Default::default()
    };

    let shutdown = StateWatcher::started();
    let mut relayer = NotInitializedTask::new(eth_node, mock_db, config, false)
        .into_task(&shutdown, ())
        .await
        .unwrap();

    relayer.page_sizer = AdaptivePageSizer::new(10, 10, 50, 10_000);
    relayer.page_sizer.successful_rpc_calls = 60; // Above threshold

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 10,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Act
    let result = relayer
        .download_logs(&eth_state.needs_to_sync_eth().unwrap())
        .await;

    // Assert - should stay at max, not exceed it
    assert!(result.is_ok());
    assert_eq!(relayer.page_sizer.page_size(), 10);
}

#[tokio::test]
async fn relayer_handles_multiple_successful_rpc_calls_per_download() {
    let eth_node = MockProvider::default();
    let logs_page1 = vec![
        Log {
            block_number: Some(2),
            ..Default::default()
        },
        Log {
            block_number: Some(3),
            ..Default::default()
        },
    ];
    let logs_page2 = vec![Log {
        block_number: Some(4),
        ..Default::default()
    }];
    eth_node.update_data(|data| data.logs_batch = vec![logs_page1, logs_page2]);

    let mock_db = crate::mock_db::MockDb::default();
    let config = Config {
        log_page_size: 10,
        ..Default::default()
    };

    let shutdown = StateWatcher::started();
    let mut relayer = NotInitializedTask::new(eth_node, mock_db, config, false)
        .into_task(&shutdown, ())
        .await
        .unwrap();

    relayer.page_sizer = AdaptivePageSizer::new(5, 10, 50, 10_000);
    relayer.page_sizer.successful_rpc_calls = 45; // Close to threshold

    let eth_state = super::state::test_builder::TestDataSource {
        eth_remote_finalized: 10,
        eth_local_finalized: 1,
    };
    let eth_state = state::build_eth(&eth_state).await.unwrap();

    // Act
    let result = relayer
        .download_logs(&eth_state.needs_to_sync_eth().unwrap())
        .await;

    // Assert
    assert!(result.is_ok());
    assert_eq!(relayer.page_sizer.successful_rpc_calls, 47); // 45 + 2
    assert_eq!(relayer.page_sizer.page_size(), 5); // Not enough to reach threshold yet
}
