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
    new_sync_state: Option<u64>,
) {
    synced.send_if_modified(|last_state| {
        if let Some(val) = new_sync_state {
            *last_state = Some(DaBlockHeight::from(val));
            true
        } else {
            false
        }
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
    // previous_state, new_state => keep waiting
    #[test_case(None, None => true; "if nothing updated with nothing then keep waiting")]
    #[test_case(None, Some(0) => false; "if nothing updated with something then stop waiting")]
    #[test_case(Some(0), Some(0) => false; "if something updated with same thing then stop waiting")]
    #[test_case(Some(0), None => true; "if something updated with nothing then keep waiting")]
    #[test_case(Some(0), Some(1) => false; "if something updated with something new then stop waiting")]
    fn can_update_sync(previous_state: Option<u64>, new_state: Option<u64>) -> bool {
        let (tx, rx) = watch::channel(previous_state.map(Into::into));
        assert!(!rx.has_changed().unwrap());
        update_synced_inner(&tx, new_state);
        !rx.has_changed().unwrap()
    }
}
