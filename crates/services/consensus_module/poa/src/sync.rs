use std::{
    sync::Arc,
    time::Duration,
};

use fuel_core_services::{
    stream::{
        BoxFuture,
        BoxStream,
    },
    RunnableService,
    RunnableTask,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    services::block_importer::BlockImportInfo,
};
use tokio::{
    sync::watch,
    time::MissedTickBehavior,
};
use tokio_stream::StreamExt;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    NotSynced,
    Synced(Arc<BlockHeader>),
}

impl SyncState {
    pub fn from_config(
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        header: &BlockHeader,
    ) -> SyncState {
        if min_connected_reserved_peers == 0 && time_until_synced == Duration::ZERO {
            SyncState::Synced(Arc::new(header.clone()))
        } else {
            SyncState::NotSynced
        }
    }
}

pub struct SyncTask {
    min_connected_reserved_peers: usize,
    peer_connections_stream: BoxStream<usize>,
    block_stream: BoxStream<BlockImportInfo>,
    state_sender: watch::Sender<SyncState>,
    // shared with `MainTask` via SyncTask::SharedState
    state_receiver: watch::Receiver<SyncState>,
    inner_state: InnerSyncState,
    timer: Option<tokio::time::Interval>,
}

impl SyncTask {
    pub fn new(
        peer_connections_stream: BoxStream<usize>,
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_stream: BoxStream<BlockImportInfo>,
        block_header: &BlockHeader,
    ) -> Self {
        let inner_state = InnerSyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_header.clone(),
        );
        let timer = if time_until_synced == Duration::ZERO {
            None
        } else {
            let mut timer = tokio::time::interval(time_until_synced);
            timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
            Some(timer)
        };

        let initial_sync_state = SyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_header,
        );

        let (state_sender, state_receiver) =
            tokio::sync::watch::channel(initial_sync_state);

        Self {
            peer_connections_stream,
            min_connected_reserved_peers,
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

    fn restart_timer(&mut self) {
        if let Some(timer) = &mut self.timer {
            timer.reset();
        }
    }
}

#[async_trait::async_trait]
impl RunnableService for SyncTask {
    const NAME: &'static str = "PoASyncTask";

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

        let tick: BoxFuture<tokio::time::Instant> = if let Some(timer) = &mut self.timer {
            Box::pin(timer.tick())
        } else {
            let future = core::future::pending();
            Box::pin(future)
        };
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                should_continue = false;
            }
            Some(latest_peer_count) = self.peer_connections_stream.next() => {
                let sufficient_peers = latest_peer_count >= self.min_connected_reserved_peers;

                match &self.inner_state {
                    InnerSyncState::InsufficientPeers(block_header) if sufficient_peers => {
                        self.inner_state = InnerSyncState::SufficientPeers(block_header.clone());
                        self.restart_timer();
                    }
                    InnerSyncState::SufficientPeers(block_header) if !sufficient_peers => {
                        self.inner_state = InnerSyncState::InsufficientPeers(block_header.clone());
                        self.restart_timer();
                    }
                    InnerSyncState::Synced { block_header, .. } => {
                        self.inner_state = InnerSyncState::Synced {
                            block_header: block_header.clone(),
                            has_sufficient_peers: sufficient_peers
                        };
                    }
                    _ => {},
                }
            }
            Some(block_info) = self.block_stream.next() => {
                let new_block_height = block_info.block_header.height();

                match &self.inner_state {
                    InnerSyncState::InsufficientPeers(block_header) if new_block_height > block_header.height() => {
                        self.inner_state = InnerSyncState::InsufficientPeers(block_info.block_header);
                    }
                    InnerSyncState::SufficientPeers(block_header) if new_block_height > block_header.height() => {
                        self.inner_state = InnerSyncState::SufficientPeers(block_info.block_header);
                        self.restart_timer();
                    }
                    InnerSyncState::Synced { block_header, has_sufficient_peers } if new_block_height > block_header.height() => {
                        if block_info.is_locally_produced() {
                            self.inner_state = InnerSyncState::Synced {
                                block_header: block_info.block_header.clone(),
                                has_sufficient_peers: *has_sufficient_peers
                            };
                            self.update_sync_state(SyncState::Synced(Arc::new(block_info.block_header)));
                        } else {
                            // we considered to be synced but we're obviously not!
                            if *has_sufficient_peers {
                                self.inner_state = InnerSyncState::SufficientPeers(block_info.block_header);
                                self.restart_timer();
                            } else {
                                self.inner_state = InnerSyncState::InsufficientPeers(block_info.block_header);
                            }

                            self.update_sync_state(SyncState::NotSynced);
                        }
                    }
                    _ => {}
                }
            }
            _ = tick => {
                if let InnerSyncState::SufficientPeers(block_header) = &self.inner_state {
                    let block_header = block_header.clone();
                    self.inner_state = InnerSyncState::Synced {
                        block_header: block_header.clone(),
                        has_sufficient_peers: true
                    };
                    self.update_sync_state(SyncState::Synced(Arc::new(block_header)));
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

#[derive(Debug, Clone)]
enum InnerSyncState {
    /// We are not connected to at least `min_connected_reserved_peers` peers.
    ///
    /// InsufficientPeers -> SufficientPeers
    InsufficientPeers(BlockHeader),
    /// We are connected to at least `min_connected_reserved_peers` peers.
    ///
    /// SufficientPeers -> Synced(...)
    /// SufficientPeers -> InsufficientPeers(...)
    SufficientPeers(BlockHeader),
    /// We can go into this state if we didn't receive any notification
    /// about new block height from the network for `time_until_synced` timeout.
    ///
    /// We can leave this state only in the case, if we received a valid block
    /// from the network with higher block height.
    ///
    /// Synced -> either InsufficientPeers(...) or SufficientPeers(...)
    Synced {
        block_header: BlockHeader,
        has_sufficient_peers: bool,
    },
}

impl InnerSyncState {
    fn from_config(
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        block_header: BlockHeader,
    ) -> Self {
        match (min_connected_reserved_peers, time_until_synced) {
            (0, Duration::ZERO) => InnerSyncState::Synced {
                block_header,
                has_sufficient_peers: true,
            },
            (0, _) => InnerSyncState::SufficientPeers(block_header),
            _ => InnerSyncState::InsufficientPeers(block_header),
        }
    }

    #[cfg(test)]
    fn block_height(&self) -> &fuel_core_types::fuel_types::BlockHeight {
        match self {
            InnerSyncState::InsufficientPeers(block_header) => block_header.height(),
            InnerSyncState::SufficientPeers(block_header) => block_header.height(),
            InnerSyncState::Synced { block_header, .. } => block_header.height(),
        }
    }
}

#[allow(clippy::arithmetic_side_effects)]
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
    use fuel_core_types::{
        fuel_types::BlockHeight,
        tai64::Tai64,
    };

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

    /// Helper function that creates a `SyncTask` with a given configuration    
    fn configure_sync_task(
        min_connected_reserved_peers: usize,
        connections_stream: impl IntoIterator<Item = usize>,
        time_until_synced: Duration,
        biggest_block: u32,
    ) -> (
        SyncTask,
        StateWatcher,
        tokio::sync::watch::Sender<fuel_core_services::State>,
    ) {
        let connections_stream = MockStream::new(connections_stream).into_boxed();

        let block_stream = MockStream::new(
            (1..biggest_block + 1)
                .map(|height| BlockHeader::new_block(height.into(), Tai64::now())),
        )
        .map(BlockImportInfo::from)
        .into_boxed();

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let watcher = shutdown.into();

        let sync_task = SyncTask::new(
            connections_stream,
            min_connected_reserved_peers,
            time_until_synced,
            block_stream,
            &Default::default(),
        );

        (sync_task, watcher, tx)
    }

    #[tokio::test]
    async fn test_sync_task() {
        // given the following config
        let connected_peers_report = 5;
        let amount_of_updates_from_stream = 1;
        let min_connected_reserved_peers = 5;
        let biggest_block = 5;
        let time_until_synced = Duration::from_secs(3);

        // and the SyncTask
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            min_connected_reserved_peers,
            vec![connected_peers_report; amount_of_updates_from_stream],
            time_until_synced,
            biggest_block,
        );

        // sync state should be NotSynced at the beginning
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
        // we should have insufficient peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::InsufficientPeers(_)
        ));

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
        matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_));

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

    // SyncTask starts with SufficientPeers and transitions back to InsufficientPeers when the peer count drops.
    #[tokio::test]
    async fn sync_task_sufficient_to_insufficient() {
        // given the following config
        let min_connected_reserved_peers = 5;
        let biggest_block = 0;
        let time_until_synced = Duration::from_secs(2);
        let connections_stream = vec![10, 4];

        // and the SyncTask
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            min_connected_reserved_peers,
            connections_stream,
            time_until_synced,
            biggest_block,
        );

        // sync state should be NotSynced at the beginning
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
        // we should have insufficient peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::InsufficientPeers(_)
        ));

        // given that we've performed a `run()` once the state should be SufficientPeers
        // since the peer count was 10
        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::SufficientPeers(_)
        ));

        // given that we've performed a `run()` again the state should be InsufficientPeers
        // since the peer count was 4
        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::InsufficientPeers(_)
        ));
    }

    // SyncTask is in Synced state and receives a block with a height greater than its current block height from the network.
    #[tokio::test]
    async fn sync_task_synced_to_greater_block_height_from_network() {
        // given the following config
        let min_connected_reserved_peers = 5;
        let biggest_block = 5;
        let time_until_synced = Duration::from_secs(2);
        let connections_stream = vec![10];

        // and the SyncTask
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            min_connected_reserved_peers,
            connections_stream.clone(),
            time_until_synced,
            biggest_block,
        );

        // given that we received all the blocks initially and peer connection updates
        for _ in 0..biggest_block as usize + connections_stream.len() {
            let _ = sync_task.run(&mut watcher).await;
        }

        // after running one more time
        let _ = sync_task.run(&mut watcher).await;

        // the state should be Synced and should also hold sufficient number of peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::Synced {
                has_sufficient_peers: true,
                ..
            }
        ));

        // given that we now added a new stream with a block height greater than the current block height
        // and the source of the new block is produced by us
        let latest_block_height = biggest_block + 1;
        let new_block_stream = MockStream::new(vec![BlockHeader::new_block(
            latest_block_height.into(),
            Tai64::now(),
        )])
        .map(BlockImportInfo::from)
        .into_boxed();
        sync_task.block_stream = new_block_stream;

        // when we run the task again
        let _ = sync_task.run(&mut watcher).await;

        // then the state should be still be Synced
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::Synced {
                has_sufficient_peers: true,
                ..
            }
        ));
        // with latest block height
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(latest_block_height)
        );
        matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_));

        // given that we now added a new stream with a block height greater than the current block height
        // and the source of the new block height is from the network
        let latest_block_height = latest_block_height + 1;
        let new_block_stream = MockStream::new(vec![BlockHeader::new_block(
            latest_block_height.into(),
            Tai64::now(),
        )])
        .map(BlockImportInfo::new_from_network)
        .into_boxed();
        sync_task.block_stream = new_block_stream;

        // when we run the task again
        let _ = sync_task.run(&mut watcher).await;

        // then the state should be SufficientPeers
        // since we have sufficient peers and the block height is greater than the current one
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::SufficientPeers(_)
        ));
        // with latest block height
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(latest_block_height)
        );
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // given now that we run the task again
        let _ = sync_task.run(&mut watcher).await;

        // we should be in Synced state again
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::Synced {
                has_sufficient_peers: true,
                ..
            }
        ));
        // with latest block height
        assert_eq!(
            sync_task.inner_state.block_height(),
            &BlockHeight::from(latest_block_height)
        );
        matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_));

        // given new stream of peer connection updates
        let new_connections_stream = MockStream::new(vec![1]).into_boxed();
        sync_task.peer_connections_stream = new_connections_stream;

        // when we run the task again
        let _ = sync_task.run(&mut watcher).await;

        // then the state should be still Synced but it should hold insufficient number of peers
        assert!(matches!(
            sync_task.inner_state,
            InnerSyncState::Synced {
                has_sufficient_peers: false,
                ..
            }
        ));
        matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_));
    }
}
