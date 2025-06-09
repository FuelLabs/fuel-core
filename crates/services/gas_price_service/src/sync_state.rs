//! This module contains the `SyncState` enum, which represents the state of the gas price synchronization process when a new L2 block is received.

pub(crate) type BlockHeight = u32;

/// The synchronization state of the gas price service.
#[derive(Debug, Clone)]
pub enum SyncState {
    /// The gas price service is not synchronized.
    NotSynced,
    /// The gas price service is synchronized to the given height.
    Synced(BlockHeight),
}

impl SyncState {
    pub(crate) fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }

    pub(crate) fn is_synced_until(&self, height: &BlockHeight) -> bool {
        matches!(self, Self::Synced(h) if h >= height)
    }
}

/// A sender for the synchronization state.
pub(crate) type SyncStateNotifier = tokio::sync::watch::Sender<SyncState>;

/// A receiver for the synchronization state.
pub type SyncStateObserver = tokio::sync::watch::Receiver<SyncState>;

pub(crate) fn new_sync_state_channel() -> (SyncStateNotifier, SyncStateObserver) {
    tokio::sync::watch::channel(SyncState::NotSynced)
}
