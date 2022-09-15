use tokio::sync::watch;

use super::state::*;

fn update_synced(synced: &watch::Sender<bool>, state: SyncState) {
    // TODO: match state to synced
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(
        EthLocal::finalized(0)
            .with_remote(EthRemote::current(200).finalization_period(100))
            .with_fuel(FuelLocal::current(0).finalized(0)),
        false
        => (false, false) ; "not synced, local eth finalize behind remote => not synced and wait"
    )]
    #[test_case(
        EthLocal::finalized(0)
            .with_remote(EthRemote::current(200).finalization_period(100))
            .with_fuel(FuelLocal::current(0).finalized(0)),
        true
        => (false, false) ; "synced, local eth finalize behind remote => not synced and wait"
    )]
    fn can_update_sync(state: SyncState, currently_synced: bool) -> (bool, bool) {
        let (tx, rx) = watch::channel(currently_synced);
        assert!(!rx.has_changed().unwrap());
        update_synced(&tx, state);
        let is_in_sync = *rx.borrow();
        (is_in_sync, rx.has_changed().unwrap())
    }
}
