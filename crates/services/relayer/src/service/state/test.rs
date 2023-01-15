use super::{
    test_builder::TestDataSource,
    *,
};

use test_case::test_case;

#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: None,
    } => Some(0..=200); "empty so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: Some(0),
    } => Some(1..=200); "behind so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: Some(200),
    } => None; "same so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: Some(201),
    } => None; "ahead so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 300,
        eth_remote_finalization_period: 100,
        eth_local_finalized: Some(50),
    } => Some(51..=200); "behind by less so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_current: 75,
        eth_remote_finalization_period: 100,
        eth_local_finalized: Some(50),
    } => None; "behind by less then finalization period so doesn't needs to sync"
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

#[test_case(EthSyncGap::new(0, 0), 0 => None)]
#[test_case(EthSyncGap::new(0, 0), 1 => Some((0, 0)))]
#[test_case(EthSyncGap::new(0, 1), 0 => None)]
#[test_case(EthSyncGap::new(0, 1), 1 => Some((0, 0)))]
#[test_case(EthSyncGap::new(0, 1), 2 => Some((0, 1)))]
#[test_case(EthSyncGap::new(31, 31), 2 => Some((31, 31)))]
#[test_case(EthSyncGap::new(30, 31), 2 => Some((30, 31)))]
#[test_case(EthSyncGap::new(29, 31), 2 => Some((29, 30)))]
#[test_case(EthSyncGap::new(28, 31), 2 => Some((28, 29)))]
fn test_page(page: EthSyncGap, size: u64) -> Option<(u64, u64)> {
    let page = page.page(size)?;
    Some((page.oldest(), page.latest()))
}

#[test_case(EthSyncGap::new(0, 0), 0, 1 => None)]
#[test_case(EthSyncGap::new(0, 0), 1, 1 => None)]
#[test_case(EthSyncGap::new(0, 1), 0, 1 => None)]
#[test_case(EthSyncGap::new(0, 1), 1, 1 => Some((1, 1)))]
#[test_case(EthSyncGap::new(0, 1), 1, 2 => None)]
#[test_case(EthSyncGap::new(0, 1), 2, 0 => Some((0, 1)))]
#[test_case(EthSyncGap::new(0, 1), 2, 1 => None)]
#[test_case(EthSyncGap::new(0, 1), 2, 2 => None)]
#[test_case(EthSyncGap::new(0, 3), 2, 0 => Some((0, 1)))]
#[test_case(EthSyncGap::new(0, 3), 2, 1 => Some((2, 3)))]
#[test_case(EthSyncGap::new(0, 3), 2, 2 => None)]
#[test_case(EthSyncGap::new(1, 8), 3, 0 => Some((1, 3)))]
#[test_case(EthSyncGap::new(1, 8), 3, 1 => Some((4, 6)))]
#[test_case(EthSyncGap::new(1, 8), 3, 2 => Some((7, 8)))]
#[test_case(EthSyncGap::new(1, 8), 3, 3 => None)]
fn test_page_reduce(page: EthSyncGap, size: u64, n: usize) -> Option<(u64, u64)> {
    let mut page = page.page(size)?;
    for _ in 0..n {
        page = page.reduce()?;
    }
    Some((page.oldest(), page.latest()))
}
