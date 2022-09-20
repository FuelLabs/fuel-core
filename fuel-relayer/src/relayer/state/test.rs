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
        fuel_local_current: 0,
        fuel_local_finalized: 0,
        fuel_remote_pending: None,
    } => false ; "local eth finalized behind remote is out of sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        fuel_local_current: 0,
        fuel_local_finalized: 0,
        fuel_remote_pending: None,
    } => true ; "local eth finalized the same as remote is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 201,
        fuel_local_current: 0,
        fuel_local_finalized: 0,
        fuel_remote_pending: None,
    } => true ; "local eth finalized is in front of remote is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        fuel_local_current: 1,
        fuel_local_finalized: 0,
        fuel_remote_pending: None,
    } => false ; "local fuel finalized is behind current is out of sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        fuel_local_current: 1,
        fuel_local_finalized: 2,
        fuel_remote_pending: None,
    } => true ; "local fuel finalized is in front of current is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        fuel_local_current: 2,
        fuel_local_finalized: 2,
        fuel_remote_pending: None,
    } => true ; "local fuel finalized is the same as current is in sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 0,
        fuel_local_current: 4,
        fuel_local_finalized: 3,
        fuel_remote_pending: None,
    } => false; "all out of sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: 200,
        fuel_local_current: 0,
        fuel_local_finalized: 0,
        fuel_remote_pending: Some(0),
    } => false ; "has pending so is out of sync"
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
        fuel_local_current: 3,
        fuel_local_finalized: 1,
        fuel_remote_pending: None,
        ..Default::default()
    } => Some(3); "behind so needs to sync"
)]
#[test_case(
    TestDataSource {
        fuel_local_current: 3,
        fuel_local_finalized: 3,
        fuel_remote_pending: None,
        ..Default::default()
    } => None; "same so in sync"
)]
#[test_case(
    TestDataSource {
        fuel_local_current: 3,
        fuel_local_finalized: 4,
        fuel_remote_pending: None,
        ..Default::default()
    } => None; "in front so in sync"
)]
#[test_case(
    TestDataSource {
        fuel_local_current: 3,
        fuel_local_finalized: 1,
        fuel_remote_pending: Some(2),
        ..Default::default()
    }=> None; "behind but has pending so in sync"
)]
#[test_case(
    TestDataSource {
        fuel_local_current: 3,
        fuel_local_finalized: 3,
        fuel_remote_pending: Some(3),
        ..Default::default()
    } => None; "same with pending so in sync"
)]
#[test_case(
    TestDataSource {
        fuel_local_current: 3,
        fuel_local_finalized: 4,
        fuel_remote_pending: Some(3),
        ..Default::default()
    } => None; "in front with pending so in sync"
)]
#[tokio::test]
async fn test_fuel_state_needs_to_publish(state: TestDataSource) -> Option<u32> {
    build_fuel(&state).await.needs_to_publish()
}
