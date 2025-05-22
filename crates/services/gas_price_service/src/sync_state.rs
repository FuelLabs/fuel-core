//! This module contains the `SyncState` enum, which represents the state of the gas price synchronization process when a new L2 block is received.

pub(crate) type BlockHeight = u32;

/// The synchronization state of the gas price.
#[derive(Debug, Clone)]
pub enum SyncState {
    /// The gas price are not synchronized.
    NotSynced,
    /// The gas price are synchronized to the given height.
    Synced(BlockHeight),
}

impl SyncState {
    pub(crate) fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }
}

/// A sender for the synchronization state.
pub(crate) type SyncStateNotifier = tokio::sync::watch::Sender<SyncState>;

/// A receiver for the synchronization state.
pub type SyncStateObserver = tokio::sync::watch::Receiver<SyncState>;
