use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};

use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    stream::BoxStream,
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::BlockHeight,
    services::{
        block_importer::BlockImportInfo,
        p2p::{
            BlockHeightHeartbeatData,
            PeerId,
        },
    },
};
use tokio::{
    sync::watch,
    time::{
        Instant,
        MissedTickBehavior,
    },
};
use tokio_stream::StreamExt;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    NotSynced,
    Synced(Arc<BlockHeader>),
}

/// True when our height gap with the network tip is 1 or less:
/// `local_height >= max_peer_heartbeat_height - 1`.
pub(crate) fn height_caught_up_to_peers(
    local_height: BlockHeight,
    max_peer_height: BlockHeight,
) -> bool {
    u32::from(local_height) >= u32::from(max_peer_height).saturating_sub(1)
}

/// Drop peer heartbeats older than this when computing the network tip.
/// ~2× default p2p `heartbeat_max_avg_interval` (20s) so a departed peer's
/// height cannot pin us `NotSynced` forever.
const PEER_HEIGHT_TTL: Duration = Duration::from_secs(40);

pub struct SyncTask {
    min_connected_reserved_peers: usize,
    peer_connections_stream: BoxStream<usize>,
    peer_height_stream: BoxStream<BlockHeightHeartbeatData>,
    peer_disconnected_stream: BoxStream<PeerId>,
    block_stream: BoxStream<BlockImportInfo>,
    state_sender: watch::Sender<SyncState>,
    // shared with `MainTask` via SyncTask::SharedState
    state_receiver: watch::Receiver<SyncState>,
    local_header: BlockHeader,
    /// Per-peer latest heartbeat height + when it was observed.
    /// Removed on disconnect (live tip) and pruned by TTL as a fallback.
    peer_heights: HashMap<PeerId, (BlockHeight, Instant)>,
    peer_count: usize,
    peer_height_ttl: Duration,
    /// Fires even when no p2p/block events arrive, so TTL pruning cannot stall
    /// while `ensure_synced` / `/v1/ready` wait on `NotSynced`.
    prune_interval: tokio::time::Interval,
}

impl SyncTask {
    pub fn new(
        peer_connections_stream: BoxStream<usize>,
        peer_height_stream: BoxStream<BlockHeightHeartbeatData>,
        peer_disconnected_stream: BoxStream<PeerId>,
        min_connected_reserved_peers: usize,
        block_stream: BoxStream<BlockImportInfo>,
        block_header: &BlockHeader,
    ) -> Self {
        let initial = if min_connected_reserved_peers == 0 {
            SyncState::Synced(Arc::new(block_header.clone()))
        } else {
            SyncState::NotSynced
        };
        let (state_sender, state_receiver) = tokio::sync::watch::channel(initial);

        // Period < TTL so a silent tip is pruned soon after expiry, not only
        // after a full extra TTL of waiting for the next tick.
        let mut prune_interval = tokio::time::interval(PEER_HEIGHT_TTL / 2);
        prune_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        Self {
            peer_connections_stream,
            peer_height_stream,
            peer_disconnected_stream,
            min_connected_reserved_peers,
            block_stream,
            state_sender,
            state_receiver,
            local_header: block_header.clone(),
            peer_heights: HashMap::new(),
            peer_count: 0,
            peer_height_ttl: PEER_HEIGHT_TTL,
            prune_interval,
        }
    }

    /// Keep the prune timer period aligned with `peer_height_ttl` (tests may
    /// shorten the TTL).
    #[cfg(test)]
    fn reset_prune_interval(&mut self) {
        let mut prune_interval = tokio::time::interval(self.peer_height_ttl / 2);
        prune_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        self.prune_interval = prune_interval;
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

    fn has_sufficient_peers(&self) -> bool {
        self.peer_count >= self.min_connected_reserved_peers
    }

    fn already_synced(&self) -> bool {
        matches!(&*self.state_receiver.borrow(), SyncState::Synced(_))
    }

    /// Max height among peers with a fresh heartbeat. Expired entries are pruned.
    fn prune_and_max_peer_height(&mut self) -> Option<BlockHeight> {
        let now = Instant::now();
        let ttl = self.peer_height_ttl;
        self.peer_heights
            .retain(|_, (_, seen)| now.saturating_duration_since(*seen) < ttl);
        self.peer_heights.values().map(|(height, _)| *height).max()
    }

    /// `min_connected_reserved_peers` is a *startup* gate to enter Synced —
    /// once Synced, a reserved-peer blip must not flip us back (matches the
    /// pre-height-gap SyncTask behavior and avoids k8s probe restart cascades).
    /// Height gap against *live* peer heartbeats can still unsync us.
    fn should_be_synced(&self, max_peer_height: Option<BlockHeight>) -> bool {
        if self.min_connected_reserved_peers == 0 {
            return true;
        }

        if self.already_synced() {
            match max_peer_height {
                Some(max_peer) => {
                    height_caught_up_to_peers(*self.local_header.height(), max_peer)
                }
                // No live tip (peer blip / all heartbeats expired) → stay Synced.
                None => true,
            }
        } else {
            self.has_sufficient_peers()
                && match max_peer_height {
                    Some(max_peer) => {
                        height_caught_up_to_peers(*self.local_header.height(), max_peer)
                    }
                    None => false,
                }
        }
    }

    fn recompute_sync_state(&mut self) {
        let max_peer_height = self.prune_and_max_peer_height();
        if self.should_be_synced(max_peer_height) {
            self.update_sync_state(SyncState::Synced(Arc::new(
                self.local_header.clone(),
            )));
        } else {
            self.update_sync_state(SyncState::NotSynced);
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
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        // Interval is ready immediately on first poll; consume that tick so
        // `run` does not spin on prune before `peer_height_ttl` elapses.
        self.prune_interval.tick().await;
        Ok(self)
    }
}

impl RunnableTask for SyncTask {
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        tokio::select! {
            biased;
            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }
            Some(latest_peer_count) = self.peer_connections_stream.next() => {
                self.peer_count = latest_peer_count;
                self.recompute_sync_state();
                TaskNextAction::Continue
            }
            Some(heartbeat) = self.peer_height_stream.next() => {
                self.peer_heights.insert(
                    heartbeat.peer_id,
                    (heartbeat.block_height, Instant::now()),
                );
                self.recompute_sync_state();
                TaskNextAction::Continue
            }
            Some(peer_id) = self.peer_disconnected_stream.next() => {
                // Drop the departed peer's tip immediately — same semantics as
                // comparing against live `all_peer_info()`, so a stale/bogus
                // height cannot pin NotSynced after disconnect.
                self.peer_heights.remove(&peer_id);
                self.recompute_sync_state();
                TaskNextAction::Continue
            }
            Some(block_info) = self.block_stream.next() => {
                if block_info.block_header.height() > self.local_header.height() {
                    self.local_header = block_info.block_header;
                    self.recompute_sync_state();
                }
                TaskNextAction::Continue
            }
            _ = self.prune_interval.tick() => {
                // No heartbeat/disconnect/block required — TTL must still run
                // or a silent high tip can permanently block ensure_synced.
                self.recompute_sync_state();
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

#[allow(clippy::arithmetic_side_effects)]
#[allow(non_snake_case)]
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
    };

    use fuel_core_services::stream::IntoBoxStream;
    use fuel_core_types::tai64::Tai64;

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

    fn peer(id: u8, height: u32) -> BlockHeightHeartbeatData {
        BlockHeightHeartbeatData {
            peer_id: PeerId::from(vec![id]),
            block_height: BlockHeight::from(height),
        }
    }

    fn configure_sync_task(
        min_connected_reserved_peers: usize,
        connections_stream: impl IntoIterator<Item = usize>,
        peer_heights: impl IntoIterator<Item = BlockHeightHeartbeatData>,
        local_blocks: impl IntoIterator<Item = u32>,
        starting_height: u32,
    ) -> (
        SyncTask,
        StateWatcher,
        tokio::sync::watch::Sender<fuel_core_services::State>,
    ) {
        let connections_stream = MockStream::new(connections_stream).into_boxed();
        let peer_height_stream = MockStream::new(peer_heights).into_boxed();
        let peer_disconnected_stream = MockStream::<PeerId>::new(vec![]).into_boxed();
        let block_stream = MockStream::new(
            local_blocks
                .into_iter()
                .map(|height| BlockHeader::new_block(height.into(), Tai64::now())),
        )
        .map(BlockImportInfo::from)
        .into_boxed();

        let (tx, shutdown) =
            tokio::sync::watch::channel(fuel_core_services::State::Started);
        let watcher = shutdown.into();

        let sync_task = SyncTask::new(
            connections_stream,
            peer_height_stream,
            peer_disconnected_stream,
            min_connected_reserved_peers,
            block_stream,
            &BlockHeader::new_block(starting_height.into(), Tai64::now()),
        );

        (sync_task, watcher, tx)
    }

    #[test]
    fn height_caught_up_to_peers__gap_of_zero_or_one_is_caught_up() {
        let local = BlockHeight::from(10u32);
        assert!(height_caught_up_to_peers(local, BlockHeight::from(10u32)));
        assert!(height_caught_up_to_peers(local, BlockHeight::from(11u32)));
        assert!(height_caught_up_to_peers(
            BlockHeight::from(12u32),
            BlockHeight::from(10u32)
        ));
    }

    #[test]
    fn height_caught_up_to_peers__gap_of_two_is_not_caught_up() {
        assert!(!height_caught_up_to_peers(
            BlockHeight::from(10u32),
            BlockHeight::from(12u32)
        ));
    }

    #[tokio::test]
    async fn sync_task__min_peers_zero_starts_synced() {
        let (sync_task, _watcher, _tx) =
            configure_sync_task(0, vec![], vec![], vec![], 5);

        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));
    }

    #[tokio::test]
    async fn sync_task__becomes_synced_when_height_gap_is_at_most_one() {
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 10)], vec![10], 9);

        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // peer count → sufficient peers, but no peer height yet
        let _ = sync_task.run(&mut watcher).await;
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // peer height 10, local still 9 → gap of 1 → Synced
        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        // local advances to 10 (optional; still Synced)
        let _ = sync_task.run(&mut watcher).await;
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));
    }

    #[tokio::test]
    async fn sync_task__stays_not_synced_when_height_gap_exceeds_one() {
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 12)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await; // peers
        let _ = sync_task.run(&mut watcher).await; // peer height

        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
    }

    #[tokio::test]
    async fn sync_task__falls_out_of_sync_when_peers_pull_ahead() {
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 10), peer(1, 13)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await; // peers
        let _ = sync_task.run(&mut watcher).await; // peer height 10 → Synced
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        let _ = sync_task.run(&mut watcher).await; // peer height 13 → gap 3 → NotSynced
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
    }

    #[tokio::test]
    async fn sync_task__insufficient_peers_keeps_not_synced() {
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(2, vec![1], vec![peer(1, 10)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await;
        let _ = sync_task.run(&mut watcher).await;

        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
    }

    #[tokio::test]
    async fn sync_task__peer_count_drop_while_synced_stays_synced() {
        // Bugbot: reserved-peer blip must not unsync / halt production / flap probes.
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 10)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await; // peers=1
        let _ = sync_task.run(&mut watcher).await; // height → Synced
        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        sync_task.peer_connections_stream = MockStream::new(vec![0]).into_boxed();
        let _ = sync_task.run(&mut watcher).await; // peers=0
        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "peer-count drop after Synced must not flip to NotSynced"
        );
    }

    #[tokio::test]
    async fn sync_task__disconnect_drops_stale_peer_height_immediately() {
        // Bugbot: departed peer's tip must not pin NotSynced (live all_peer_info semantics).
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 100), peer(2, 10)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await; // peers=1
        let _ = sync_task.run(&mut watcher).await; // peer1@100 → NotSynced (gap)
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        let _ = sync_task.run(&mut watcher).await; // peer2@10 (still NotSynced; max=100)
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        sync_task.peer_disconnected_stream =
            MockStream::new(vec![PeerId::from(vec![1])]).into_boxed();
        let _ = sync_task.run(&mut watcher).await; // disconnect peer1 → max=10 → Synced

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "disconnect must drop the departed peer tip immediately"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sync_task__ttl_timer_prunes_without_other_events() {
        // High tip from peer 1 blocks sync; peer 2 is caught up. No further
        // heartbeats/disconnects/blocks arrive — only the prune timer can
        // drop peer 1 after TTL and unblock Synced.
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(1, 100), peer(2, 10)], vec![], 10);

        let _ = sync_task.run(&mut watcher).await; // peers
        let _ = sync_task.run(&mut watcher).await; // peer1@100 → NotSynced
        let _ = sync_task.run(&mut watcher).await; // peer2@10; max still 100
        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());

        // peer1 already past TTL; peer2 must remain fresh after the advance
        // (`age < ttl`) or both tips prune and we stay NotSynced with no tip.
        let peer1 = PeerId::from(vec![1]);
        sync_task.peer_heights.get_mut(&peer1).expect("peer1").1 = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .expect("clock");
        sync_task.peer_height_ttl = Duration::from_secs(5);
        sync_task.reset_prune_interval();
        sync_task.prune_interval.tick().await; // discard immediate first tick

        tokio::time::advance(Duration::from_secs(2)).await;
        let _ = sync_task.run(&mut watcher).await; // prune timer only

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "prune timer must drop the stale high tip without other events"
        );
    }

    #[tokio::test]
    async fn sync_task__expired_peer_height_does_not_block_sync_forever() {
        // Bugbot: monotonic forever-max of departed peers must not pin NotSynced.
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(1, vec![1], vec![peer(2, 10)], vec![], 10);

        // Seed a stale, unreachable height from a departed peer.
        sync_task.peer_height_ttl = Duration::from_millis(1);
        sync_task.reset_prune_interval();
        sync_task.peer_heights.insert(
            PeerId::from(vec![1]),
            (
                BlockHeight::from(100u32),
                Instant::now()
                    .checked_sub(Duration::from_secs(60))
                    .expect("clock"),
            ),
        );

        let _ = sync_task.run(&mut watcher).await; // peers
        // Live peer at 10; stale 100 must be pruned → Synced
        let _ = sync_task.run(&mut watcher).await;

        assert!(
            matches!(*sync_task.state_receiver.borrow(), SyncState::Synced(_)),
            "expired peer heights must not pin NotSynced"
        );
    }

    #[tokio::test]
    async fn sync_task__network_block_while_synced_with_min_peers_zero_stays_synced() {
        // Regression for the old quiet-window + Source::Network flap that
        // deadlocked ensure_synced during reconciliation. With height-gap
        // sync and min_peers=0, importing a network block just advances
        // local height and stays Synced.
        let (mut sync_task, mut watcher, _tx) =
            configure_sync_task(0, vec![], vec![], vec![], 5);

        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));

        sync_task.block_stream =
            MockStream::new(vec![BlockHeader::new_block(6u32.into(), Tai64::now())])
                .map(BlockImportInfo::new_from_network)
                .into_boxed();

        let _ = sync_task.run(&mut watcher).await;

        assert!(matches!(
            *sync_task.state_receiver.borrow(),
            SyncState::Synced(_)
        ));
        assert_eq!(sync_task.local_header.height(), &BlockHeight::from(6u32));
    }
}
