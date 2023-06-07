use std::time::Duration;

use fuel_core_services::{
    stream::BoxStream,
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_core_types::fuel_types::BlockHeight;
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
    block_stream: BoxStream<BlockHeight>,
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
        block_stream: BoxStream<BlockHeight>,
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

    async fn update_timeout(&mut self) {
        if self.inner_state.is_sufficient() {
            self.timer
                .set_timeout(self.time_until_synced, OnConflict::Overwrite)
                .await;
        } else {
            self.timer.clear().await;
        }
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
            latest_count = self.peer_connections_stream.next() => {
                if let Some(latest_count) = latest_count {
                    if self.state_sender.send_if_modified(|sync_state: &mut SyncState| {
                        if self.inner_state.change_state_on_peers_update(latest_count, self.min_connected_reserved_peers) {
                            *sync_state = self.inner_state.sync_state();
                            true
                        } else {
                            false
                        }
                    }) {
                        self.update_timeout().await;
                    }
                }

            }
            block = self.block_stream.next() => {
                if let Some(new_block_height) = block {
                    self.state_sender.send_if_modified(|sync_state: &mut SyncState| {
                        if self.inner_state.change_state_on_block(new_block_height) {
                            *sync_state = self.inner_state.sync_state();
                            true
                        } else {
                            false
                        }
                    });

                    // update timeout on each received block
                    self.update_timeout().await;

                }
            }
            _ = self.timer.wait() => {
                self.state_sender.send_if_modified(|sync_state: &mut SyncState| {
                    if self.inner_state.change_on_sync_timeout() {
                        *sync_state = self.inner_state.sync_state();
                        true
                    } else {
                        false
                    }
                });
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
    fn from_config(
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_height: BlockHeight,
    ) -> Self {
        match (min_connected_reserved_peers, time_until_synced) {
            (0, Duration::ZERO) => InnerSyncState::Synced(block_height),
            (0, _) => InnerSyncState::SufficientPeers(block_height),
            _ => InnerSyncState::InsufficientPeers(block_height),
        }
    }

    fn change_state_on_block(&mut self, new_block_height: BlockHeight) -> bool {
        let mut state_changed = false;
        let current_block_height = self.block_height();

        if &new_block_height > current_block_height {
            match self {
                InnerSyncState::InsufficientPeers(_) => {
                    *self = InnerSyncState::InsufficientPeers(new_block_height);
                }
                InnerSyncState::SufficientPeers(_) => {
                    *self = InnerSyncState::SufficientPeers(new_block_height);
                }
                InnerSyncState::Synced(_) => {
                    // we considered to be synced but we're obviously not!
                    *self = InnerSyncState::SufficientPeers(new_block_height);
                    state_changed = true;
                }
            }
        }

        state_changed
    }

    fn change_state_on_peers_update(
        &mut self,
        currently_connected_peers: usize,
        min_connected_reserved_peers: usize,
    ) -> bool {
        let sufficient_peers = currently_connected_peers >= min_connected_reserved_peers;

        match self {
            InnerSyncState::InsufficientPeers(block_height) if sufficient_peers => {
                *self = InnerSyncState::SufficientPeers(*block_height);
                true
            }
            InnerSyncState::SufficientPeers(block_height) if !sufficient_peers => {
                *self = InnerSyncState::InsufficientPeers(*block_height);
                true
            }
            _ => false,
        }
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

    fn block_height(&self) -> &BlockHeight {
        match self {
            InnerSyncState::InsufficientPeers(block_height)
            | InnerSyncState::SufficientPeers(block_height)
            | InnerSyncState::Synced(block_height) => block_height,
        }
    }

    fn is_sufficient(&self) -> bool {
        match self {
            InnerSyncState::InsufficientPeers(_) | InnerSyncState::Synced(_) => false,
            InnerSyncState::SufficientPeers(_) => true,
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

    #[test]
    fn test_inner_sync_state() {
        // given that we set `min_connected_reserved_peers` to be 0
        // and `time_until_synced` to finish immediately
        let min_connected_reserved_peers = 0;
        let time_until_synced = Duration::ZERO;
        let block_height = BlockHeight::default();

        let inner_sync_state = InnerSyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_height,
        );

        // inner state should be synced
        assert!(matches!(inner_sync_state, InnerSyncState::Synced(_)));
        assert_eq!(*inner_sync_state.block_height(), block_height);
        assert_eq!(inner_sync_state.sync_state(), SyncState::Synced);

        // given that `time_until_synced` has been increased
        // yet `min_connected_reserved_peers` is still 0
        let time_until_synced = Duration::from_secs(10);

        let inner_sync_state = InnerSyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_height,
        );

        // we should have sufficient number of peers but not synced yet,
        // ie `time_until_synced` has not completed yet
        assert!(inner_sync_state.is_sufficient());
        assert_eq!(inner_sync_state.sync_state(), SyncState::NotSynced);

        // given that we now increased `min_connected_reserved_peers`
        // `time_until_synced` is still 10 seconds
        let min_connected_reserved_peers = 10;
        let inner_sync_state = InnerSyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_height,
        );

        // we should be in InsufficientPeers state
        // since we neither have enough peers nor `time_until_synced` has completed
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::InsufficientPeers(_)
        ));
        assert_eq!(inner_sync_state.sync_state(), SyncState::NotSynced);
    }

    #[test]
    fn test_inner_sync_state_change_state_on_peers_update() {
        // given that we start from InssuficientPeers state
        let block_height = BlockHeight::default();
        let mut inner_sync_state = InnerSyncState::InsufficientPeers(block_height);

        let min_connected_reserved_peers = 10;
        let connected_peers = 0;

        // and that we still don't have enough peers connected
        assert!(!inner_sync_state
            .change_state_on_peers_update(connected_peers, min_connected_reserved_peers));

        // we should be still at insufficient peers
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::InsufficientPeers(_)
        ));

        // given that now our peer count has increased
        let connected_peers = 10;
        assert!(inner_sync_state
            .change_state_on_peers_update(connected_peers, min_connected_reserved_peers));

        // we should move to SufficientPeers state
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::SufficientPeers(_)
        ));

        // given that our peer count has decreased
        let connected_peers = 9;
        assert!(inner_sync_state
            .change_state_on_peers_update(connected_peers, min_connected_reserved_peers));

        // we should move back to InsufficientPeers state
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::InsufficientPeers(_)
        ));
    }

    #[test]
    fn test_inner_sync_state_change_state_on_block() {
        // given that we start from InssuficientPeers state
        let block_height = BlockHeight::default();
        let mut inner_sync_state = InnerSyncState::InsufficientPeers(block_height);

        // and that we receive a new, bigger block height
        let new_block_height = BlockHeight::from(10);

        // the state should remain the same
        assert!(!inner_sync_state.change_state_on_block(new_block_height));
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::InsufficientPeers(_)
        ));
        // but the block height should be updated
        assert_eq!(&new_block_height, inner_sync_state.block_height());

        // given that we moved to SufficientPeers state
        let mut inner_sync_state = InnerSyncState::SufficientPeers(new_block_height);

        // and that we receive a new, bigger block height
        let new_block_height = BlockHeight::from(11);

        // the state should remain the same
        assert!(!inner_sync_state.change_state_on_block(new_block_height));
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::SufficientPeers(_)
        ));
        // but the block height should be updated
        assert_eq!(&new_block_height, inner_sync_state.block_height());

        // given that we moved to Synced state
        let mut inner_sync_state = InnerSyncState::Synced(new_block_height);

        // and that we receive a new, bigger block height
        let new_block_height = BlockHeight::from(12);

        // the state should change
        // since this would mean we're not synced with other peers
        assert!(inner_sync_state.change_state_on_block(new_block_height));
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::SufficientPeers(_)
        ));
        // the block height should be updated
        assert_eq!(&new_block_height, inner_sync_state.block_height());
    }

    #[test]
    fn test_inner_sync_state_change_on_timeout() {
        // given that we start from InssuficientPeers state
        let block_height = BlockHeight::default();
        let mut inner_sync_state = InnerSyncState::InsufficientPeers(block_height);

        // the state should remain the same if sync_timeout happened
        assert!(!inner_sync_state.change_on_sync_timeout());
        assert!(matches!(
            inner_sync_state,
            InnerSyncState::InsufficientPeers(_)
        ));

        // given that we moved to SufficientPeers state
        let mut inner_sync_state = InnerSyncState::SufficientPeers(block_height);

        // the state should move to Synced
        // since the timeout is used to go from Syncing -> Synced
        assert!(inner_sync_state.change_on_sync_timeout());
        assert!(matches!(inner_sync_state, InnerSyncState::Synced(_)));

        // given that we're now in Synced state
        // the state should remain the same if sync_timeout happened
        assert!(!inner_sync_state.change_on_sync_timeout());
        assert!(matches!(inner_sync_state, InnerSyncState::Synced(_)));
    }

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
        let block_stream =
            MockStream::new((1..biggest_block + 1).map(BlockHeight::from)).into_boxed();

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
        assert!(matches!(sync_task.inner_state, InnerSyncState::Synced(_)));

        // and the block should be still the latest one
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(biggest_block)
        );
    }
}
