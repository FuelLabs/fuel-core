//! Synced State
//! Handles the logic for updating the [`RelayerHandle`](crate::RelayerHandle)
//! if the relayer has reached a consistent state with the DA layer.

use fuel_core_types::blockchain::primitives::DaBlockHeight;
use tokio::sync::watch;

use super::{
    state::*,
    NotifySynced,
};

/// Notify the handle if the state is synced with the DA layer.
pub fn update_synced(synced: &NotifySynced, state: &EthState) {
    update_synced_inner(synced, state.is_synced_at())
}

/// Updates the sender state but only notifies if the
/// state has become synced.
fn update_synced_inner(
    synced: &watch::Sender<Option<DaBlockHeight>>,
    is_synced: Option<u64>,
) {
    synced.send_if_modified(|last_state| {
        let r = is_synced.is_some();
        *last_state = is_synced.map(DaBlockHeight::from);
        r
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // The input is the sync state change of the relayer and
    // on the result is the `RelayerHandle` observed state.
    //
    // `should_wait` means calls to `await_synced` will yield
    // until a future state change that puts the relayer in sync
    // with the ethereum node.
    //
    // previous_state, new_state => (state, should_wait)
    #[test_case(false, false => (false, true))]
    #[test_case(false, true => (true, false))]
    #[test_case(true, true => (true, false))]
    #[test_case(true, false => (false, true))]
    fn can_update_sync(was_synced: bool, is_synced: bool) -> (bool, bool) {
        let (tx, rx) = watch::channel(was_synced.then_some(0u64.into()));
        assert!(!rx.has_changed().unwrap());
        update_synced_inner(&tx, is_synced.then_some(0u64));
        let is_in_sync = rx.borrow().is_some();
        (is_in_sync, !rx.has_changed().unwrap())
    }
}
