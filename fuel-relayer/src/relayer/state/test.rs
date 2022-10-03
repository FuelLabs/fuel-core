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

#[test_case(EthSyncGap::new(0, 0), 0 => (0, 0, true))]
#[test_case(EthSyncGap::new(0, 0), 1 => (0, 0, false))]
#[test_case(EthSyncGap::new(0, 1), 0 => (0, 0, true))]
#[test_case(EthSyncGap::new(0, 1), 1 => (0, 0, false))]
#[test_case(EthSyncGap::new(0, 1), 2 => (0, 1, false))]
#[test_case(EthSyncGap::new(31, 31), 2 => (31, 31, false))]
#[test_case(EthSyncGap::new(30, 31), 2 => (30, 31, false))]
#[test_case(EthSyncGap::new(29, 31), 2 => (29, 30, false))]
#[test_case(EthSyncGap::new(28, 31), 2 => (28, 29, false))]
fn test_page(page: EthSyncGap, size: u64) -> (u64, u64, bool) {
    let page = page.page(size);
    (page.oldest(), page.latest(), page.is_empty())
}

#[test_case(EthSyncGap::new(0, 0), 0, 1 => (0, 0, true))]
#[test_case(EthSyncGap::new(0, 0), 1, 1 => (1, 0, true))]
#[test_case(EthSyncGap::new(0, 1), 0, 1 => (0, 0, true))]
#[test_case(EthSyncGap::new(0, 1), 1, 1 => (1, 1, false))]
#[test_case(EthSyncGap::new(0, 1), 1, 2 => (2, 1, true))]
#[test_case(EthSyncGap::new(0, 1), 2, 0 => (0, 1, false))]
#[test_case(EthSyncGap::new(0, 1), 2, 1 => (2, 1, true))]
#[test_case(EthSyncGap::new(0, 1), 2, 2 => (4, 1, true))]
#[test_case(EthSyncGap::new(0, 3), 2, 0 => (0, 1, false))]
#[test_case(EthSyncGap::new(0, 3), 2, 1 => (2, 3, false))]
#[test_case(EthSyncGap::new(0, 3), 2, 2 => (4, 3, true))]
#[test_case(EthSyncGap::new(1, 8), 3, 0 => (1, 3, false))]
#[test_case(EthSyncGap::new(1, 8), 3, 1 => (4, 6, false))]
#[test_case(EthSyncGap::new(1, 8), 3, 2 => (7, 8, false))]
#[test_case(EthSyncGap::new(1, 8), 3, 3 => (10, 8, true))]
fn test_page_reduce(page: EthSyncGap, size: u64, n: usize) -> (u64, u64, bool) {
    let mut page = page.page(size);
    for _ in 0..n {
        page.reduce();
    }
    (page.oldest(), page.latest(), page.is_empty())
}
