use super::*;

use test_case::test_case;

#[test_case(
    EthLocal::finalized(0)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(0).finalized(0))
    => false ; "local eth finalized behind remote is out of sync"
)]
#[test_case(
    EthLocal::finalized(200)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(0).finalized(0))
    => true ; "local eth finalized the same as remote is in sync"
)]
#[test_case(
    EthLocal::finalized(201)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(0).finalized(0))
    => true ; "local eth finalized is in front of remote is in sync"
)]
#[test_case(
    EthLocal::finalized(200)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(1).finalized(0))
    => false ; "local fuel finalized is behind current is out of sync"
)]
#[test_case(
    EthLocal::finalized(200)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(1).finalized(2))
    => true ; "local fuel finalized is in front of current is in sync"
)]
#[test_case(
    EthLocal::finalized(200)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(2).finalized(2))
    => true ; "local fuel finalized is the same as current is in sync"
)]
#[test_case(
    EthLocal::finalized(0)
        .with_remote(EthRemote::current(300).finalization_period(100))
        .with_fuel(FuelLocal::current(4).finalized(3))
    => false; "all out of sync"
)]
fn test_sync_state_is_synced(state: SyncState) -> bool {
    state.is_synced()
}

#[test_case(
    EthLocal::finalized(0)
        .with_remote(EthRemote::current(300).finalization_period(100))
    => Some(0..=200); "behind so needs to sync"
)]
#[test_case(
    EthLocal::finalized(200)
        .with_remote(EthRemote::current(300).finalization_period(100))
    => None; "same so doesn't need to sync"
)]
#[test_case(
    EthLocal::finalized(201)
        .with_remote(EthRemote::current(300).finalization_period(100))
    => None; "ahead so doesn't need to sync"
)]
#[test_case(
    EthLocal::finalized(50)
        .with_remote(EthRemote::current(300).finalization_period(100))
    => Some(50..=200); "behind by less so needs to sync"
)]
fn test_eth_state_needs_to_sync_eth(state: EthState) -> Option<RangeInclusive<u64>> {
    state.needs_to_sync_eth().map(|g| g.0 .0)
}

#[test_case(
    FuelLocal::finalized(1).current(3)
    => Some(3); "behind so needs to sync"
)]
#[test_case(
    FuelLocal::finalized(3).current(3)
    => None; "same so in sync"
)]
#[test_case(
    FuelLocal::finalized(4).current(3)
    => None; "in front so in sync"
)]
fn test_fuel_state_needs_to_publish(state: FuelState) -> Option<u32> {
    state.needs_to_publish()
}
