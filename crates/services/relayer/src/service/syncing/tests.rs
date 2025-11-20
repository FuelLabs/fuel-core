use alloy_rpc_types_eth::SyncInfo;
use fuel_core_provider::test_helpers::provider::{MockProvider, TriggerType};
use fuel_core_relayer as _;
use std::ops::RangeInclusive;

use super::*;
use test_case::test_case;

#[tokio::test(start_paused = true)]
async fn handles_syncing() {
    let eth_node = MockProvider::default();
    eth_node.update_data(|data| {
        let status = SyncInfo {
            starting_block: U256::from(100),
            current_block: U256::from(0),
            highest_block: U256::from(130),
            warp_chunks_amount: None,
            warp_chunks_processed: None,
            stages: None,
        };
        data.is_syncing = SyncStatus::Info(Box::new(status));
    });

    let mut count = 0;
    eth_node.set_before_event(move |data, evt| {
        if let TriggerType::Syncing = evt {
            count += 1;
            if count == 30 {
                data.is_syncing = SyncStatus::None;
            } else {
                let status = SyncInfo {
                    starting_block: U256::from(100),
                    current_block: U256::from(100 + count),
                    highest_block: U256::from(130),
                    warp_chunks_amount: None,
                    warp_chunks_processed: None,
                    stages: None,
                };
                data.is_syncing = SyncStatus::Info(Box::new(status));
            }
        }
    });

    let before = tokio::time::Instant::now();
    wait_if_eth_syncing(&eth_node, Duration::from_secs(5), Duration::from_secs(10))
        .await
        .unwrap();
    let after = tokio::time::Instant::now();

    assert_eq!(
        before.elapsed().checked_sub(after.elapsed()),
        Some(Duration::from_secs(5) * 29)
    );
}

#[allow(clippy::reversed_empty_ranges)]
#[test_case(status(0..=0, 0) => "from 0 to 0 currently at 0. 100% Done.")]
#[test_case(status(0..=100, 0) => "from 0 to 100 currently at 0. 0% Done.")]
#[test_case(status(50..=100, 75) => "from 50 to 100 currently at 75. 50% Done.")]
#[test_case(status(50..=150, 150) => "from 50 to 150 currently at 150. 100% Done.")]
#[test_case(status(50..=49, 50) => "from 50 to 49 currently at 50. 100% Done.")]
#[test_case(status(50..=50, 51) => "from 50 to 50 currently at 51. 100% Done.")]
fn test_status(s: Status) -> String {
    s.to_string()
}

fn status(r: RangeInclusive<u64>, c: u64) -> Status {
    Status {
        starting_block: U256::from(*r.start()),
        highest_block: U256::from(*r.end()),
        current_block: U256::from(c),
    }
}
