use super::{
    test_builder::TestDataSource,
    *,
};

use test_case::test_case;

#[test_case(
    TestDataSource {
        eth_remote_finalized: 200,
        eth_local_finalized: None,
    } => Some(0..=200); "empty so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_finalized: 200,
        eth_local_finalized: Some(0),
    } => Some(1..=200); "behind so needs to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_finalized: 200,
        eth_local_finalized: Some(200),
    } => None; "same so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_finalized: 200,
        eth_local_finalized: Some(201),
    } => None; "ahead so doesn't need to sync"
)]
#[test_case(
    TestDataSource {
        eth_remote_finalized: 200,
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
        .map(Into::into)
}

#[test_case(EthSyncGap::new(0, 0), 0 => None; "0 page size results in no page with no gap")]
#[test_case(EthSyncGap::new(0, 0), 1 => Some((0, 0)); "page includes 0 to 0 when size is 1")]
#[test_case(EthSyncGap::new(0, 1), 0 => None; "0 page size results in no page with gap")]
#[test_case(EthSyncGap::new(0, 1), 1 => Some((0, 0)); "gap shrunk to size 1 with size 1")]
#[test_case(EthSyncGap::new(0, 1), 2 => Some((0, 1)); "page matches gap when size is the same")]
#[test_case(EthSyncGap::new(31, 31), 2 => Some((31, 31)); "page includes 31 to 31 when size is 2")]
#[test_case(EthSyncGap::new(30, 31), 2 => Some((30, 31)); "page includes 30 to 31 when size is 2")]
#[test_case(EthSyncGap::new(29, 31), 2 => Some((29, 30)); "for 29 to 31 page only includes 29 to 30 when size is 2")]
#[test_case(EthSyncGap::new(28, 31), 2 => Some((28, 29)); "for 28 to 31 page only includes 28 to 29 when size is 2")]
fn test_page(eth_sync_gap: EthSyncGap, size: u64) -> Option<(u64, u64)> {
    let page = eth_sync_gap.page(size)?;
    Some((page.oldest(), page.latest()))
}

#[test_case(EthSyncGap::new(0, 0), 1, 1 => None; "size one page can't be reduced")]
#[test_case(EthSyncGap::new(0, 1), 1, 1 => Some((1, 1)); "page of size 1 for gap of size 2 can be reduced")]
#[test_case(EthSyncGap::new(0, 1), 1, 2 => None; "page of size 1 for gap of size 2 can't be reduced twice")]
#[test_case(EthSyncGap::new(0, 1), 2, 1 => None; "page of size 2 for gap of size 2 can't be reduced")]
#[test_case(EthSyncGap::new(0, 3), 2, 1 => Some((2, 3)); "page of size 2 for gap of size 4 can be reduced")]
#[test_case(EthSyncGap::new(0, 3), 2, 2 => None; "page of size 2 for gap of size 4 can't be reduced twice")]
#[test_case(EthSyncGap::new(1, 8), 3, 1 => Some((4, 6)); "page of size 3 for gap of size 8 can be reduced")]
#[test_case(EthSyncGap::new(1, 8), 3, 2 => Some((7, 8)); "page of size 3 for gap of size 8 can be reduced twice")]
#[test_case(EthSyncGap::new(1, 8), 3, 3 => None; "page of size 3 for gap of size 8 can't be reduced three times")]
fn test_page_reduce(
    eth_page_gap: EthSyncGap,
    size: u64,
    reduce_count: usize,
) -> Option<(u64, u64)> {
    let mut page = eth_page_gap.page(size)?;
    for _ in 0..reduce_count {
        page = page.reduce()?;
    }
    Some((page.oldest(), page.latest()))
}
