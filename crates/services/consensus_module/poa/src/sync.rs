use std::time::Duration;

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_core_types::{
    fuel_types::BlockHeight,
    services::block_importer::BlockImportInfo,
};

use tokio::sync::watch;
use tokio_stream::StreamExt;

use crate::deadline_clock::{
    DeadlineClock,
    OnConflict,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SyncState {
    NotSynced,
    Synced,
}

impl SyncState {
    pub fn from_config(
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
    ) -> SyncState {
        if min_connected_reserved_peers == 0 && time_until_synced == Duration::ZERO {
            SyncState::Synced
        } else {
            SyncState::NotSynced
        }
    }
}

pub struct SyncTask {
    min_connected_reserved_peers: usize,
    time_until_synced: Duration,
    peer_connections_stream: BoxStream<usize>,
    block_stream: BoxStream<BlockImportInfo>,
    state_sender: watch::Sender<SyncState>,
    // shared with `MainTask` via SyncTask::SharedState
    state_receiver: watch::Receiver<SyncState>,
    inner_state: InnerSyncState,
    timer: DeadlineClock,
}

impl SyncTask {
    pub fn new(
        peer_connections_stream: BoxStream<usize>,
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_stream: BoxStream<BlockImportInfo>,
        block_height: BlockHeight,
    ) -> Self {
        let inner_state = InnerSyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_height,
        );
        let timer = DeadlineClock::new();

        let initial_sync_state =
            SyncState::from_config(min_connected_reserved_peers, time_until_synced);

        let (state_sender, state_receiver) =
            tokio::sync::watch::channel(initial_sync_state);

        Self {
            peer_connections_stream,
            min_connected_reserved_peers,
            time_until_synced,
            block_stream,
            state_sender,
            state_receiver,
            inner_state,
            timer,
        }
    }

    fn update_sync_state(&mut self, new_state: SyncState) {
        self.state_sender
            .send_if_modified(|sync_state: &mut SyncState| {
                if new_state == *sync_state {
                    false
                } else {
                    *sync_state = new_state;
                    true
                }
            });
    }

    async fn restart_timer(&mut self) {
        self.timer
            .set_timeout(self.time_until_synced, OnConflict::Overwrite)
            .await;
    }
}

#[async_trait::async_trait]
impl RunnableService for SyncTask {
    const NAME: &'static str = "fuel-core-consensus/poa/sync-task";

    type SharedData = watch::Receiver<SyncState>;
    type TaskParams = ();

    type Task = SyncTask;

    fn shared_data(&self) -> Self::SharedData {
        self.state_receiver.clone()
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl RunnableTask for SyncTask {
    #[tracing::instrument(level = "debug", skip_all, err, ret)]
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let mut should_continue = true;

        tokio::select! {
            _ = watcher.while_started() => {
                should_continue = false;
            }
            Some(latest_peer_count) = self.peer_connections_stream.next() => {
                let sufficient_peers = latest_peer_count >= self.min_connected_reserved_peers;

                match self.inner_state {
                    InnerSyncState::InsufficientPeers(block_height) if sufficient_peers => {
                        self.inner_state = InnerSyncState::SufficientPeers(block_height);
                        self.restart_timer().await;
                    }
                    InnerSyncState::SufficientPeers(block_height) if !sufficient_peers => {
                        self.inner_state = InnerSyncState::InsufficientPeers(block_height);
                        self.timer.clear().await;
                    }
                    InnerSyncState::Synced { block_height, .. } => {
                        self.inner_state = InnerSyncState::Synced { block_height, has_sufficient_peers: sufficient_peers };
                    }
                    _ => {},
                }
            }
            Some(block_info) = self.block_stream.next() => {
                let new_block_height = block_info.height;

                match self.inner_state {
                    InnerSyncState::InsufficientPeers(block_height) if new_block_height > block_height => {
                        self.inner_state = InnerSyncState::InsufficientPeers(new_block_height);
                    }
                    InnerSyncState::SufficientPeers(block_height) if new_block_height > block_height => {
                        self.inner_state = InnerSyncState::SufficientPeers(new_block_height);
                        self.restart_timer().await;
                    }
                    InnerSyncState::Synced { block_height, has_sufficient_peers } if new_block_height > block_height => {
                        if block_info.is_locally_produced() {
                            self.inner_state = InnerSyncState::Synced { block_height: new_block_height, has_sufficient_peers };
                        } else {
                            // we considered to be synced but we're obviously not!
                            if has_sufficient_peers {
                                self.inner_state = InnerSyncState::SufficientPeers(new_block_height);
                                self.restart_timer().await;
                            } else {
                                self.inner_state = InnerSyncState::InsufficientPeers(new_block_height);
                            }

                            self.update_sync_state(SyncState::NotSynced);
                        }
                    }
                    _ => {}
                }
            }
            _ = self.timer.wait() => {
                if let InnerSyncState::SufficientPeers(block_height) = self.inner_state {
                    self.inner_state = InnerSyncState::Synced { block_height, has_sufficient_peers: true };
                    self.update_sync_state(SyncState::Synced);
                }
            }
        }

        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        // Nothing to shut down because we don't have any temporary state that should be dumped,
        // and we don't spawn any sub-tasks that we need to finish or await.
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
    /// SufficientPeers -> InsufficientPeers(...)
    SufficientPeers(BlockHeight),
    /// We can go into this state if we didn't receive any notification
    /// about new block height from the network for `time_until_synced` timeout.
    ///
    /// We can leave this state only in the case, if we received a valid block
    /// from the network with higher block height.
    ///
    /// Synced -> either InsufficientPeers(...) or SufficientPeers(...)
    Synced {
        block_height: BlockHeight,
        has_sufficient_peers: bool,
    },
}

impl InnerSyncState {
    fn from_config(
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_height: BlockHeight,
    ) -> Self {
        match (min_connected_reserved_peers, time_until_synced) {
            (0, Duration::ZERO) => InnerSyncState::Synced {
                block_height,
                has_sufficient_peers: true,
            },
            (0, _) => InnerSyncState::SufficientPeers(block_height),
            _ => InnerSyncState::InsufficientPeers(block_height),
        }
    }

    #[cfg(test)]
    fn block_height(&self) -> &BlockHeight {
        match self {
            InnerSyncState::InsufficientPeers(block_height) => block_height,
            InnerSyncState::SufficientPeers(block_height) => block_height,
            InnerSyncState::Synced { block_height, .. } => block_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        pin::Pin,
        task::{
            Context,
            Poll,
        },
        time::Duration,
    };

    use fuel_core_services::stream::IntoBoxStream;

    struct MockStream<T> {
        items: VecDeque<T>,
    }

    impl<T> MockStream<T> {
        fn new(range: impl IntoIterator<Item = T>) -> Self {
            Self {
                items: range.into_iter().collect(),
            }
        }
    }

    impl<T> tokio_stream::Stream for MockStream<T>
    where
        T: Unpin,
    {
        type Item = T;

        fn poll_next(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            if this.items.is_empty() {
                Poll::Pending
            } else {
                let next_item = this.items.pop_front();
                Poll::Ready(next_item)
            }
        }
    }

    #[tokio::test]
    async fn test_sync_task() {
        // given the following config
        let connected_peers_report = 5;
        let amount_of_updates_from_stream = 1;
        let min_connected_reserved_peers = 5;
        let biggest_block = 5;
        let time_until_synced = Duration::from_secs(5);

        let connections_stream =
            MockStream::new(vec![connected_peers_report; amount_of_updates_from_stream])
                .into_boxed();
        let block_stream = MockStream::new((1..biggest_block + 1).map(BlockHeight::from))
            .map(BlockImportInfo::from)
            .into_boxed();

        // and the SyncTask
        let mut sync_task = SyncTask::new(
            connections_stream,
            min_connected_reserved_peers,
            time_until_synced,
            block_stream,
            BlockHeight::default(),
        );

        // sync state should be NotSynced at the beginning
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
        // we should have insufficient peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::InsufficientPeers(_)
        ));

        let (_tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher = shutdown.into();

        // given that we've performed a `run()` `amount_of_updates_from_stream + biggest_block` times
        let run_times = amount_of_updates_from_stream + biggest_block as usize;
        for _ in 0..run_times {
            let _ = sync_task.run(&mut watcher).await;
        }

        // the state should still be NotSynced
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // but we should have sufficient peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::SufficientPeers(_)
        ));

        // and the block should be the latest one
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(biggest_block)
        );

        // given that we now run the task again
        // both block stream and p2p connected peers updates stream would be empty
        // hence the timeout should activate and expire
        let _ = sync_task.run(&mut watcher).await;

        // at that point we should be in Synced state
        assert_eq!(SyncState::Synced, *sync_task.state_receiver.borrow());

        // synced should reflect here as well
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::Synced { .. }
        ));

        // and the block should be still the latest one
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(biggest_block)
        );
    }
}
