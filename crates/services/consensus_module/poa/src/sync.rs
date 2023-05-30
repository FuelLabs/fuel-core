use std::{
    sync::Arc,
    time::Duration,
};

use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::ImportResult,
};
use tokio::{
    sync::{
        broadcast,
        watch,
    },
    time::Instant,
};

use crate::ports::P2pPort;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncState {
    NotSynced,
    Synced,
}

pub struct SyncTask<T> {
    min_connected_reserved_peers: usize,
    time_until_synced: Duration,
    time_left_until_synced: Duration,
    p2p_port: T,
    block_rx: broadcast::Receiver<Arc<ImportResult>>,
    state_sender: watch::Sender<SyncState>,
    inner_state: InnerSyncState,
    notify: Arc<tokio::sync::Notify>,
}

impl<T> SyncTask<T> {
    pub fn new(
        p2p_port: T,
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_rx: broadcast::Receiver<Arc<ImportResult>>,
        notify: Arc<tokio::sync::Notify>,
        state_sender: watch::Sender<SyncState>,
        block_height: BlockHeight,
    ) -> Self {
        let inner_state = {
            match (min_connected_reserved_peers, time_until_synced) {
                (0, Duration::ZERO) => InnerSyncState::Synced(block_height),
                (0, _) => InnerSyncState::SufficientPeers(block_height),
                _ => InnerSyncState::InsufficientPeers(block_height),
            }
        };

        Self {
            min_connected_reserved_peers,
            time_until_synced,
            time_left_until_synced: time_until_synced,
            p2p_port,
            block_rx,
            state_sender,
            inner_state,
            notify,
        }
    }
}

impl<T> SyncTask<T>
where
    T: P2pPort,
{
    pub async fn run(&mut self) -> anyhow::Result<()> {
        if self.inner_state.is_sufficient() {
            self.sync_when_sufficient().await?;
        } else {
            self.sync().await?;
        }

        Ok(())
    }

    async fn sync(&mut self) -> anyhow::Result<()> {
        let mut reserved_peers_count = self.p2p_port.reserved_peers_count();

        tokio::select! {
            latest_count = reserved_peers_count.recv() => {
                if let Ok(latest_count) = latest_count {
                    if self.inner_state.change_state_on_peers_update(latest_count >= self.min_connected_reserved_peers) {
                        self.state_sender.send(self.inner_state.sync_state())?;
                    }
                }
            }
            block = self.block_rx.recv() => {
                if let Ok(block) = block {
                    if self.inner_state.change_state_on_block(block) {
                        self.state_sender.send(self.inner_state.sync_state())?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn sync_when_sufficient(&mut self) -> anyhow::Result<()> {
        let mut reserved_peers_count = self.p2p_port.reserved_peers_count();
        let now = Instant::now();

        tokio::select! {
            latest_count = reserved_peers_count.recv() => {
                if let Ok(latest_count) = latest_count {
                    if self.inner_state.change_state_on_peers_update(latest_count >= self.min_connected_reserved_peers) {
                        self.time_left_until_synced = self.time_until_synced;
                        self.state_sender.send(self.inner_state.sync_state())?;
                    } else {
                        self.time_left_until_synced = self.time_left_until_synced - now.elapsed();
                    }
                }

            }
            block = self.block_rx.recv() => {
                if let Ok(block) = block {
                    if self.inner_state.change_state_on_block(block) {
                        self.time_left_until_synced = self.time_until_synced;
                        self.state_sender.send(self.inner_state.sync_state())?;
                    } else {
                        self.time_left_until_synced = self.time_left_until_synced - now.elapsed();
                    }
                }
            }
            _ = tokio::time::sleep(self.time_left_until_synced) => {
                if self.inner_state.change_on_sync_timeout() {
                    self.time_left_until_synced = self.time_until_synced;
                    self.notify.notify_one();
                    self.state_sender.send(self.inner_state.sync_state())?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum InnerSyncState {
    /// We are not connected to at least `min_connected_reserved_peers` peers.
    ///
    /// InsufficientPeers -> SufficientPeers
    InsufficientPeers(BlockHeight),
    /// We are connected to at least `min_connected_reserved_peers` peers.
    ///
    /// SufficientPeers -> Synced(...)
    SufficientPeers(BlockHeight),
    /// We can go into this state if we didn't receive any notification
    /// about new block height from the network for `time_until_synced` timeout.
    ///
    /// We can leave this state only in the case, if we received a valid block
    /// from the network with higher block height.
    ///
    /// Synced -> either InsufficientPeers(...) or SufficientPeers(...)
    Synced(BlockHeight),
}

impl InnerSyncState {
    fn change_state_on_block(&mut self, block: Arc<ImportResult>) -> bool {
        let mut state_changed = false;

        let latest_block_height = block.sealed_block.entity.header().height();
        let current_block_height = self.block_height();

        if latest_block_height > current_block_height {
            match self {
                InnerSyncState::InsufficientPeers(_) => {
                    *self = InnerSyncState::InsufficientPeers(*latest_block_height);
                }
                InnerSyncState::SufficientPeers(_) => {
                    *self = InnerSyncState::SufficientPeers(*latest_block_height);
                }
                InnerSyncState::Synced(_) => {
                    // we considered to be synced but we're obiously not!
                    *self = InnerSyncState::SufficientPeers(*latest_block_height);
                    state_changed = true;
                }
            }
        }

        state_changed
    }

    fn change_state_on_peers_update(&mut self, sufficient_peers: bool) -> bool {
        let mut state_changed = false;

        match self {
            InnerSyncState::InsufficientPeers(block_height) => {
                if sufficient_peers {
                    *self = InnerSyncState::SufficientPeers(*block_height);
                    state_changed = true;
                }
            }
            InnerSyncState::SufficientPeers(block_height)
            | InnerSyncState::Synced(block_height) => {
                if !sufficient_peers {
                    *self = InnerSyncState::InsufficientPeers(*block_height);
                    state_changed = true;
                }
            }
        }

        state_changed
    }

    fn change_on_sync_timeout(&mut self) -> bool {
        match self {
            InnerSyncState::SufficientPeers(block_height) => {
                *self = InnerSyncState::Synced(*block_height);
                true
            }
            _ => false,
        }
    }

    fn sync_state(&self) -> SyncState {
        match self {
            InnerSyncState::InsufficientPeers(_) | InnerSyncState::SufficientPeers(_) => {
                SyncState::NotSynced
            }
            InnerSyncState::Synced(_) => SyncState::Synced,
        }
    }

    fn block_height(&self) -> &BlockHeight {
        match self {
            InnerSyncState::InsufficientPeers(block_height)
            | InnerSyncState::SufficientPeers(block_height)
            | InnerSyncState::Synced(block_height) => &block_height,
        }
    }

    fn is_sufficient(&self) -> bool {
        match self {
            InnerSyncState::InsufficientPeers(_) | InnerSyncState::Synced(_) => false,
            InnerSyncState::SufficientPeers(_) => true,
        }
    }
}
