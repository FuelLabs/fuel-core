//! This module contains the `SyncState` enum, which represents the state of the synchronization process between storages.

use crate::ports::block_source::BlockHeight;

/// The synchronization state between storages.
#[derive(Debug, Clone)]
pub enum SyncState {
    /// The storages are not synchronized.
    NotSynced,
    /// The storages are synchronized to the given height.
    Synced(BlockHeight),
}

impl SyncState {
    pub(crate) fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }
}

/// A receiver for the synchronization state.
pub type SyncStateObserver = tokio::sync::watch::Receiver<SyncState>;
/// A sender for the synchronization state.
pub(crate) type SyncStateNotifier = tokio::sync::watch::Sender<SyncState>;

pub(crate) fn new_sync_state_channel() -> (SyncStateNotifier, SyncStateObserver) {
    tokio::sync::watch::channel(SyncState::NotSynced)
}
