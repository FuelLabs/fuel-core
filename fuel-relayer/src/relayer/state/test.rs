use super::{
    test_builder::TestDataSource,
    *,
};

use test_case::test_case;

#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 0,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(0)),
        num_unpublished_messages: 0,
    } => false ; "local eth finalized behind remote is out of sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(0)),
        num_unpublished_messages: 0,
    } => true ; "local eth finalized the same as remote is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 201,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(0)),
        num_unpublished_messages: 0,
    } => true ; "local eth finalized is in front of remote is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 0,
    } => true; "time window elapsed but no unpublished messages"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 1,
    } => false; "time window elapsed and one unpublished messages"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(0)),
        num_unpublished_messages: 11,
    } => false; "enough messages to force published"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 11,
    } => false; "enough messages to force published and time elapsed"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(1)),
        num_unpublished_messages: 1,
    } => true; "not enough messages to force published and time window not elapsed"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 0,
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 20,
    } => false; "all out of sync"
)]
#[tokio::test]
async fn test_sync_state_is_synced(state: TestDataSource) -> bool {
    build(&state).await.unwrap().is_synced()
}

#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 0,
        ..Default::default()
    } => Some(0..=200); "behind so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        ..Default::default()
    } => None; "same so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 201,
        ..Default::default()
    } => None; "ahead so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 50,
        ..Default::default()
    } => Some(50..=200); "behind by less so needs to sync"
)]
#[tokio::test]
async fn test_eth_state_needs_to_sync_eth(
    state: TestDataSource,
) -> Option<RangeInclusive<u64>> {
    build_eth(&state)
        .await
        .unwrap()
        .needs_to_sync_eth()
        .map(|g| g.0 .0)
}

#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 0,
        ..Default::default()
    } => false; "time window elapsed but no unpublished messages"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 1,
        ..Default::default()
    } => true; "time window elapsed and one unpublished messages"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(0)),
        num_unpublished_messages: 11,
        ..Default::default()
    } => true; "enough messages to force published"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(20)),
        num_unpublished_messages: 11,
        ..Default::default()
    } => true; "enough messages to force published and time elapsed"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: Some(Duration::from_secs(1)),
        num_unpublished_messages: 1,
        ..Default::default()
    } => false; "not enough messages to force published and time window not elapsed"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: None,
        latest_block_time: None,
        num_unpublished_messages: 0,
        ..Default::default()
    } => false; "No times but no messages"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: None,
        latest_block_time: None,
        num_unpublished_messages: 1,
        ..Default::default()
    } => true; "No times"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: Some(Duration::from_secs(0)),
        latest_block_time: None,
        num_unpublished_messages: 1,
        ..Default::default()
    } => true; "Only last sent"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: None,
        latest_block_time: Some(Duration::from_secs(1)),
        num_unpublished_messages: 1,
        ..Default::default()
    } => false; "Only latest but not past window"
)]
#[test_case(
    TestDataSource {
        message_time_window: Duration::from_secs(10),
        min_messages_to_force_publish: 10,
        last_sent_time: None,
        latest_block_time: Some(Duration::from_secs(11)),
        num_unpublished_messages: 1,
        ..Default::default()
    } => true; "Only latest but past window"
)]
#[tokio::test]
async fn test_fuel_state_needs_to_publish(state: TestDataSource) -> bool {
    build_fuel(&state).await.needs_to_publish()
}
