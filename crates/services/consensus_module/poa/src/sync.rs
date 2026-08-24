use std::{
    sync::{
        Arc,
        atomic::{
            AtomicU32,
            Ordering,
        },
    },
    time::Duration,
};

use crate::ports::{
    BlockImporter,
    P2pPort,
};
use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    stream::{
        BoxFuture,
        BoxStream,
    },
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::BlockHeight,
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

/// Height Gap = reserved-peer network height − local DB height.
/// Isolated/dev (`min_connected_reserved_peers == 0`) skips the comparison.
pub(crate) fn height_gap_is_ready(
    min_connected_reserved_peers: usize,
    max_sync_height_diff: u32,
    reserved_peer_network_height: Option<BlockHeight>,
    local_height: Option<BlockHeight>,
) -> bool {
    if min_connected_reserved_peers == 0 {
        return true;
    }
    let Some(network) = reserved_peer_network_height else {
        return false;
    };
    let local = local_height.unwrap_or(BlockHeight::from(0u32));
    let gap = u32::from(network).saturating_sub(u32::from(local));
    gap <= max_sync_height_diff
}

/// While Synced, re-snapshot P2P vs DB on this interval so `SyncState` tracks Green's
/// live height-gap definition (entry and exit), including when peer heartbeats advance
/// the network tip without a reserved-peer count or block-import event.
const SYNCED_HEIGHT_GAP_RECHECK_INTERVAL: Duration = Duration::from_secs(10);

pub struct SyncTask {
    min_connected_reserved_peers: usize,
    max_sync_height_diff: u32,
    time_until_synced: Duration,
    peer_connections_stream: BoxStream<usize>,
    block_stream: BoxStream<BlockImportInfo>,
    p2p: Arc<dyn P2pPort>,
    block_importer: Arc<dyn BlockImporter>,
    state_sender: watch::Sender<SyncState>,
    // shared with `MainTask` via SyncTask::SharedState
    state_receiver: watch::Receiver<SyncState>,
    inner_state: InnerSyncState,
    timer: Option<tokio::time::Interval>,
    /// True after `restart_timer` once the height gap is satisfied; the tick arm
    /// only advances to Synced when armed so a stale interval from startup cannot
    /// skip `--time-until-synced`.
    debounce_armed: bool,
    /// Re-snapshot reserved-peer height vs DB while Synced so `SyncState` tracks
    /// the live height-gap definition (not a one-shot latch on entry).
    synced_recheck: tokio::time::Interval,
    /// Blocks at heights <= this watermark were imported via reconciliation
    /// by the leader and should not trigger Synced → NotSynced transitions.
    /// Set by MainTask via `fetch_max`, monotonically increasing, never cleared.
    reconciliation_watermark: Arc<AtomicU32>,
}

impl SyncTask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        peer_connections_stream: BoxStream<usize>,
        min_connected_reserved_peers: usize,
        time_until_synced: Duration,
        max_sync_height_diff: u32,
        block_stream: BoxStream<BlockImportInfo>,
        block_header: &BlockHeader,
        reconciliation_watermark: Arc<AtomicU32>,
        p2p: Arc<dyn P2pPort>,
        block_importer: Arc<dyn BlockImporter>,
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
            // First Interval tick is immediate — reset so debounce only starts
            // when we arm it after the height gap is ready.
            timer.reset();
            Some(timer)
        };

        let initial_sync_state = SyncState::from_config(
            min_connected_reserved_peers,
            time_until_synced,
            block_header,
        );

        let (state_sender, state_receiver) =
            tokio::sync::watch::channel(initial_sync_state);

        let mut synced_recheck =
            tokio::time::interval(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL);
        synced_recheck.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Don't fire immediately on startup — give P2P heartbeats time to arrive.
        synced_recheck.reset();

        Self {
            peer_connections_stream,
            min_connected_reserved_peers,
            max_sync_height_diff,
            time_until_synced,
            block_stream,
            p2p,
            block_importer,
            state_sender,
            state_receiver,
            inner_state,
            timer,
            debounce_armed: false,
            synced_recheck,
            reconciliation_watermark,
        }
    }

    async fn height_gap_ready(&self) -> bool {
        if self.min_connected_reserved_peers == 0 {
            return true;
        }
        let network = self.p2p.reserved_peer_network_height().await;
        let local = self.block_importer.latest_block_height().ok().flatten();
        height_gap_is_ready(
            self.min_connected_reserved_peers,
            self.max_sync_height_diff,
            network,
            local,
        )
    }

    async fn advance_to_synced_if_ready(&mut self) {
        let InnerSyncState::SufficientPeers(block_header) = self.inner_state.clone()
        else {
            return;
        };
        if !self.height_gap_ready().await {
            // Gap lost before the debounce fired — require a fresh window.
            self.debounce_armed = false;
            return;
        }
        self.debounce_armed = false;
        self.inner_state = InnerSyncState::Synced {
            block_header: block_header.clone(),
            has_sufficient_peers: true,
        };
        self.update_sync_state(SyncState::Synced(Arc::new(block_header)));
    }

    async fn on_sufficient_peers_activity(&mut self) {
        if !matches!(self.inner_state, InnerSyncState::SufficientPeers(_)) {
            return;
        }
        if !self.height_gap_ready().await {
            self.debounce_armed = false;
            return;
        }
        if self.time_until_synced == Duration::ZERO {
            self.advance_to_synced_if_ready().await;
        } else if !self.debounce_armed {
            // Arm once; recheck must not reset_timer every 10s or a long
            // --time-until-synced never completes.
            self.restart_timer();
            self.debounce_armed = true;
        }
    }

    /// Leave Synced when the live height gap exceeds `--max-sync-height-diff`.
    /// Peer-count blips alone do not flip Synced (legacy behaviour preserved).
    async fn recompute_if_synced_lagging(&mut self) {
        let InnerSyncState::Synced {
            block_header,
            has_sufficient_peers,
        } = self.inner_state.clone()
        else {
            return;
        };
        if self.height_gap_ready().await {
            return;
        }
        self.inner_state = if has_sufficient_peers {
            InnerSyncState::SufficientPeers(block_header)
        } else {
            InnerSyncState::InsufficientPeers(block_header)
        };
        self.update_sync_state(SyncState::NotSynced);
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

impl RunnableTask for SyncTask {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        let tick: BoxFuture<tokio::time::Instant> = match &mut self.timer {
            Some(timer) => Box::pin(timer.tick()),
            _ => {
                let future = core::future::pending();
                Box::pin(future)
            }
        };
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            Some(latest_peer_count) = self.peer_connections_stream.next() => {
                let sufficient_peers = latest_peer_count >= self.min_connected_reserved_peers;

                match &self.inner_state {
                    InnerSyncState::InsufficientPeers(block_header) if sufficient_peers => {
                        self.inner_state = InnerSyncState::SufficientPeers(block_header.clone());
                    }
                    InnerSyncState::SufficientPeers(block_header) if !sufficient_peers => {
                        self.inner_state = InnerSyncState::InsufficientPeers(block_header.clone());
                    }
                    InnerSyncState::Synced { block_header, .. } => {
                        self.inner_state = InnerSyncState::Synced {
                            block_header: block_header.clone(),
                            has_sufficient_peers: sufficient_peers
                        };
                    }
                    _ => {},
                }
                self.on_sufficient_peers_activity().await;
                self.recompute_if_synced_lagging().await;
                TaskNextAction::Continue
            }
            Some(block_info) = self.block_stream.next() => {
                let new_block_height = block_info.block_header.height();

                match &self.inner_state {
                    InnerSyncState::InsufficientPeers(block_header) if new_block_height > block_header.height() => {
                        self.inner_state = InnerSyncState::InsufficientPeers(block_info.block_header);
                    }
                    InnerSyncState::SufficientPeers(block_header) if new_block_height > block_header.height() => {
                        self.inner_state = InnerSyncState::SufficientPeers(block_info.block_header);
                        self.on_sufficient_peers_activity().await;
                    }
                    InnerSyncState::Synced { block_header, has_sufficient_peers } if new_block_height > block_header.height() => {
                        let watermark = self.reconciliation_watermark.load(Ordering::Acquire);
                        let is_reconciliation = watermark > 0
                            && u32::from(*new_block_height) <= watermark;

                        if block_info.is_locally_produced() || is_reconciliation {
                            self.inner_state = InnerSyncState::Synced {
                                block_header: block_info.block_header.clone(),
                                has_sufficient_peers: *has_sufficient_peers
                            };
                            self.update_sync_state(SyncState::Synced(Arc::new(block_info.block_header)));
                        } else {
                            // A network import does not itself mean the node is lagging.
                            // The live height-gap check below decides whether to leave Synced.
                            self.inner_state = InnerSyncState::Synced {
                                block_header: block_info.block_header.clone(),
                                has_sufficient_peers: *has_sufficient_peers,
                            };
                            self.update_sync_state(SyncState::Synced(Arc::new(
                                block_info.block_header,
                            )));
                        }
                    }
                    _ => {}
                }
                self.recompute_if_synced_lagging().await;
                TaskNextAction::Continue
            }
            _ = tick => {
                // Debounce elapsed only after height gap armed the timer.
                if self.debounce_armed {
                    self.advance_to_synced_if_ready().await;
                }
                self.recompute_if_synced_lagging().await;
                TaskNextAction::Continue
            }
            _ = self.synced_recheck.tick() => {
                // Live P2P snapshot (Green): heartbeats can satisfy the gap while we
                // sit in SufficientPeers. Respect --time-until-synced via
                // on_sufficient_peers_activity only — do not call
                // advance_to_synced_if_ready here (that skips the debounce).
                self.on_sufficient_peers_activity().await;
                self.recompute_if_synced_lagging().await;
                TaskNextAction::Continue
            }
        }
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
    /// Height gap must be within `--max-sync-height-diff`; then `time_until_synced`
    /// debounce applies (immediate when zero).
    SufficientPeers(BlockHeader),
    /// We can go into this state once the height gap is within threshold and
    /// `time_until_synced` has elapsed (or is zero).
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
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        MockBlockImporter,
        MockP2pPort,
    };
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

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(move || Ok(Some(BlockHeight::from(biggest_block))));

        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                Box::pin(async move { Some(BlockHeight::from(biggest_block)) })
            });

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let watcher = shutdown.into();

        let sync_task = SyncTask::new(
            connections_stream,
            min_connected_reserved_peers,
            time_until_synced,
            1,
            block_stream,
            &Default::default(),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
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

    /// Without a reconciliation watermark, a network-sourced block leaves Synced.
    /// Height-gap re-entry is blocked here by requiring reserved peers with no
    /// heartbeat — so published state stays NotSynced (legacy deadlock case for
    /// `ensure_synced` when the gap cannot recover immediately).
    #[tokio::test]
    async fn sync_task__network_block_at_reconciliation_height_causes_not_synced_without_watermark()
     {
        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let watermark = Arc::new(AtomicU32::new(0));

        let mut p2p = MockP2pPort::new();
        // Heartbeat present while catching up to Synced, then cleared so a
        // network block cannot immediately re-enter via height gap.
        let heartbeat = Arc::new(std::sync::Mutex::new(Some(BlockHeight::from(5u32))));
        let heartbeat_for_mock = Arc::clone(&heartbeat);
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *heartbeat_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(5u32))));

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            Duration::ZERO,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            watermark,
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        *heartbeat.lock().unwrap() = None;

        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(6u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();
        let _ = sync_task.run(&mut watcher).await;

        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "Without watermark, a network-sourced block leaves Synced when the \
             height gap cannot re-satisfy immediately"
        );

        drop(tx);
    }

    /// Verifies the watermark fix: when the reconciliation watermark covers
    /// the block height, a network-sourced block should NOT trigger NotSynced.
    #[tokio::test]
    async fn sync_task__network_block_within_watermark_stays_synced() {
        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let watermark = Arc::new(AtomicU32::new(6));

        let mut p2p = MockP2pPort::new();
        let heartbeat = Arc::new(std::sync::Mutex::new(Some(BlockHeight::from(5u32))));
        let heartbeat_for_mock = Arc::clone(&heartbeat);
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *heartbeat_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(5u32))));

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            Duration::ZERO,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            watermark,
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        // when: a Source::Network block at height 6 (within watermark).
        // Keep heartbeat so recompute_if_synced_lagging does not drop Synced after
        // the watermark path keeps us synced.
        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(6u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();
        let _ = sync_task.run(&mut watcher).await;

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "With watermark=6, a network block at height 6 should NOT trigger NotSynced"
        );

        // when: a Source::Network block at height 7 (ABOVE watermark).
        // Clear heartbeat so height-gap cannot immediately re-enter Synced.
        *heartbeat.lock().unwrap() = None;
        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(7u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();
        let _ = sync_task.run(&mut watcher).await;

        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "A network block above the watermark should still trigger NotSynced"
        );

        drop(tx);
    }

    #[test]
    fn height_gap_is_ready__min_peers_zero_skips_comparison() {
        assert!(height_gap_is_ready(0, 1, None, None));
        assert!(height_gap_is_ready(
            0,
            1,
            Some(BlockHeight::from(100u32)),
            Some(BlockHeight::from(0u32))
        ));
    }

    #[test]
    fn height_gap_is_ready__no_heartbeat_is_not_ready() {
        assert!(!height_gap_is_ready(
            1,
            1,
            None,
            Some(BlockHeight::from(10u32))
        ));
    }

    #[test]
    fn height_gap_is_ready__gap_within_max_is_ready() {
        let local = Some(BlockHeight::from(10u32));
        assert!(height_gap_is_ready(
            1,
            1,
            Some(BlockHeight::from(10u32)),
            local
        ));
        assert!(height_gap_is_ready(
            1,
            1,
            Some(BlockHeight::from(11u32)),
            local
        ));
        assert!(height_gap_is_ready(
            1,
            1,
            Some(BlockHeight::from(5u32)),
            local
        ));
    }

    #[test]
    fn height_gap_is_ready__gap_above_max_is_not_ready() {
        assert!(!height_gap_is_ready(
            1,
            1,
            Some(BlockHeight::from(12u32)),
            Some(BlockHeight::from(10u32))
        ));
        assert!(height_gap_is_ready(
            1,
            2,
            Some(BlockHeight::from(12u32)),
            Some(BlockHeight::from(10u32))
        ));
    }

    /// Synced must track the live height-gap definition (Green): if peers advance
    /// while local DB stalls, SyncTask clears SyncState even without a new block.
    #[tokio::test]
    async fn sync_task_synced_leaves_synced_when_height_gap_grows() {
        use std::sync::Mutex;

        let connections_stream = MockStream::new(vec![5, 5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let network_height = Arc::new(Mutex::new(Some(BlockHeight::from(5u32))));
        let network_height_for_mock = Arc::clone(&network_height);
        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *network_height_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let local_height = BlockHeight::from(5u32);
        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(move || Ok(Some(local_height)));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            Duration::ZERO,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        *network_height.lock().unwrap() = Some(BlockHeight::from(10u32));
        let _ = sync_task.run(&mut watcher).await;

        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "Reserved-peer tip advanced while local DB stalled — must leave Synced"
        );

        drop(tx);
    }

    /// Heartbeat-only gap satisfaction must enter Synced via the live P2P recheck arm
    /// (DEVOPS-1520 follower case: peer count stable, first heartbeat arrives later).
    #[tokio::test(start_paused = true)]
    async fn sync_task_sufficient_peers_enters_synced_on_heartbeat_recheck() {
        use std::sync::Mutex;

        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let network_height = Arc::new(Mutex::new(None::<BlockHeight>));
        let network_height_for_mock = Arc::clone(&network_height);
        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *network_height_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let local_height = BlockHeight::from(5u32);
        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(move || Ok(Some(local_height)));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            Duration::ZERO,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "No heartbeat height yet — must stay NotSynced"
        );

        *network_height.lock().unwrap() = Some(BlockHeight::from(5u32));
        tokio::time::advance(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL).await;
        let _ = sync_task.run(&mut watcher).await;

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "Heartbeat satisfied height gap — recheck must enter Synced"
        );

        drop(tx);
    }

    /// Network block while Synced must not leave watch channel on NotSynced while
    /// inner_state is Synced (Bugbot: on_sufficient_peers_activity before NotSynced).
    #[tokio::test]
    async fn sync_task_network_block_keeps_inner_and_published_state_consistent() {
        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let mut p2p = MockP2pPort::new();
        // After importing height 6, gap vs network tip 6 is 0 → ready to re-enter Synced.
        p2p.expect_reserved_peer_network_height()
            .returning(|| Box::pin(async { Some(BlockHeight::from(6u32)) }));

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(6u32))));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            Duration::ZERO,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(6u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();
        let _ = sync_task.run(&mut watcher).await;

        let published = sync_task.state_receiver.borrow().clone();
        let inner_synced = matches!(sync_task.inner_state, InnerSyncState::Synced { .. });
        let published_synced = matches!(published, SyncState::Synced(_));
        assert_eq!(
            inner_synced, published_synced,
            "inner_state and published SyncState must agree after network block \
             (got inner_synced={inner_synced}, published={published:?})"
        );
        assert!(
            published_synced,
            "With time_until_synced=0 and gap still ok after import, must re-enter Synced"
        );

        drop(tx);
    }

    #[tokio::test(start_paused = true)]
    async fn sync_task_network_block_within_height_gap_does_not_restart_debounce() {
        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(|| Box::pin(async { Some(BlockHeight::from(6u32)) }));

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(6u32))));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();
        let debounce = Duration::from_secs(2);

        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            debounce,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert!(sync_task.debounce_armed);

        tokio::time::advance(debounce).await;
        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));
        assert!(!sync_task.debounce_armed);

        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(6u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();
        let _ = sync_task.run(&mut watcher).await;

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "A network import within the allowed height gap must stay Synced"
        );
        assert!(
            !sync_task.debounce_armed,
            "Staying within the height gap must not restart the debounce"
        );

        drop(tx);
    }

    /// Recheck arm must not skip --time-until-synced (Bugbot: advance_to_synced_if_ready
    /// on the recheck path ignored the debounce).
    #[tokio::test(start_paused = true)]
    async fn sync_task_recheck_respects_time_until_synced_debounce() {
        use std::sync::Mutex;

        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let network_height = Arc::new(Mutex::new(None::<BlockHeight>));
        let network_height_for_mock = Arc::clone(&network_height);
        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *network_height_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(5u32))));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let debounce = Duration::from_secs(2);
        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            debounce,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        *network_height.lock().unwrap() = Some(BlockHeight::from(5u32));
        // Drain select arms: a stale timer tick may run before recheck arms debounce.
        tokio::time::advance(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL).await;
        for _ in 0..4 {
            let _ = sync_task.run(&mut watcher).await;
            if sync_task.debounce_armed {
                break;
            }
        }
        assert!(
            sync_task.debounce_armed,
            "Recheck must arm debounce once the height gap is satisfied"
        );
        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "Recheck must not enter Synced before time_until_synced elapses"
        );

        tokio::time::advance(debounce).await;
        let _ = sync_task.run(&mut watcher).await;
        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "After time_until_synced elapses, tick arm must enter Synced"
        );

        drop(tx);
    }

    /// Recheck must not `restart_timer` while debounce is already armed
    /// (Bugbot: --time-until-synced > 10s never completed).
    #[tokio::test(start_paused = true)]
    async fn sync_task_recheck_does_not_restart_armed_debounce() {
        use std::sync::Mutex;

        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let network_height = Arc::new(Mutex::new(None::<BlockHeight>));
        let network_height_for_mock = Arc::clone(&network_height);
        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *network_height_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(5u32))));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        // Longer than the 10s recheck interval so a reset would prevent Synced.
        let debounce = Duration::from_secs(15);
        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            debounce,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        let _ = sync_task.run(&mut watcher).await;
        *network_height.lock().unwrap() = Some(BlockHeight::from(5u32));

        tokio::time::advance(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL).await;
        for _ in 0..4 {
            let _ = sync_task.run(&mut watcher).await;
            if sync_task.debounce_armed {
                break;
            }
        }
        assert!(sync_task.debounce_armed);

        // Another full recheck cycle while still gap-ready must not reset the timer.
        tokio::time::advance(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL).await;
        let _ = sync_task.run(&mut watcher).await;
        assert!(
            sync_task.debounce_armed,
            "Gap still ready: debounce stays armed without restart"
        );
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // Remaining debounce after one recheck (15s - 10s = 5s) should finish.
        tokio::time::advance(Duration::from_secs(5)).await;
        let _ = sync_task.run(&mut watcher).await;
        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "Debounce must complete despite intervening rechecks"
        );

        drop(tx);
    }

    /// A debounce tick that fails the height-gap check must disarm so the next
    /// tick cannot enter Synced without a fresh --time-until-synced window.
    #[tokio::test(start_paused = true)]
    async fn sync_task_failed_debounce_tick_disarms() {
        use std::sync::Mutex;

        let connections_stream = MockStream::new(vec![5]).into_boxed();
        let block_stream = MockStream::<BlockImportInfo>::new(vec![]).into_boxed();

        let network_height = Arc::new(Mutex::new(None::<BlockHeight>));
        let network_height_for_mock = Arc::clone(&network_height);
        let mut p2p = MockP2pPort::new();
        p2p.expect_reserved_peer_network_height()
            .returning(move || {
                let height = *network_height_for_mock.lock().unwrap();
                Box::pin(async move { height })
            });

        let mut block_importer = MockBlockImporter::new();
        block_importer
            .expect_latest_block_height()
            .returning(|| Ok(Some(BlockHeight::from(5u32))));

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let mut watcher: StateWatcher = shutdown.into();

        let debounce = Duration::from_secs(2);
        let mut sync_task = SyncTask::new(
            connections_stream,
            5,
            debounce,
            1,
            block_stream,
            &BlockHeader::new_block(5u32.into(), Tai64::now()),
            Arc::new(AtomicU32::new(0)),
            Arc::new(p2p),
            Arc::new(block_importer),
        );

        // Enter SufficientPeers with gap not yet ready.
        let _ = sync_task.run(&mut watcher).await;
        *network_height.lock().unwrap() = Some(BlockHeight::from(5u32));

        tokio::time::advance(SYNCED_HEIGHT_GAP_RECHECK_INTERVAL).await;
        for _ in 0..4 {
            let _ = sync_task.run(&mut watcher).await;
            if sync_task.debounce_armed {
                break;
            }
        }
        assert!(sync_task.debounce_armed);

        // Gap widens before the debounce fires.
        *network_height.lock().unwrap() = Some(BlockHeight::from(100u32));
        tokio::time::advance(debounce).await;
        let _ = sync_task.run(&mut watcher).await;
        assert!(
            !sync_task.debounce_armed,
            "Failed advance must clear debounce_armed"
        );
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // Gap recovers; without a new arm, a stray timer tick must not Synced.
        *network_height.lock().unwrap() = Some(BlockHeight::from(5u32));
        tokio::time::advance(debounce).await;
        let _ = sync_task.run(&mut watcher).await;
        assert_eq!(
            SyncState::NotSynced,
            *sync_task.state_receiver.borrow(),
            "Must not Synced on tick while disarmed"
        );

        drop(tx);
    }
}
