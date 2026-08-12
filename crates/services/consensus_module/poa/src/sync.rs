use std::sync::Arc;

use fuel_core_services::{
    RunnableService,
    RunnableTask,
    StateWatcher,
    TaskNextAction,
    stream::{
        BoxStream,
    },
};
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::BlockHeight,
    services::block_importer::BlockImportInfo,
};
use tokio::sync::watch;
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

pub struct SyncTask {
    min_connected_reserved_peers: usize,
    peer_connections_stream: BoxStream<usize>,
    peer_height_stream: BoxStream<BlockHeight>,
    block_stream: BoxStream<BlockImportInfo>,
    state_sender: watch::Sender<SyncState>,
    // shared with `MainTask` via SyncTask::SharedState
    state_receiver: watch::Receiver<SyncState>,
    local_header: BlockHeader,
    max_peer_height: Option<BlockHeight>,
    peer_count: usize,
}

impl SyncTask {
    pub fn new(
        peer_connections_stream: BoxStream<usize>,
        peer_height_stream: BoxStream<BlockHeight>,
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

        Self {
            peer_connections_stream,
            peer_height_stream,
            min_connected_reserved_peers,
            block_stream,
            state_sender,
            state_receiver,
            local_header: block_header.clone(),
            max_peer_height: None,
            peer_count: 0,
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

    fn has_sufficient_peers(&self) -> bool {
        self.peer_count >= self.min_connected_reserved_peers
    }

    /// Synced when we have enough reserved peers and our height gap with the
    /// network is ≤ 1. `min_connected_reserved_peers == 0` means isolated/dev
    /// mode: there is no network tip to compare against, so we are ready.
    fn should_be_synced(&self) -> bool {
        if !self.has_sufficient_peers() {
            return false;
        }
        if self.min_connected_reserved_peers == 0 {
            return true;
        }
        match self.max_peer_height {
            Some(max_peer) => {
                height_caught_up_to_peers(*self.local_header.height(), max_peer)
            }
            None => false,
        }
    }

    fn recompute_sync_state(&mut self) {
        if self.should_be_synced() {
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
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
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
            Some(peer_height) = self.peer_height_stream.next() => {
                self.max_peer_height = Some(
                    self.max_peer_height
                        .map_or(peer_height, |max| max.max(peer_height)),
                );
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

    fn configure_sync_task(
        min_connected_reserved_peers: usize,
        connections_stream: impl IntoIterator<Item = usize>,
        peer_heights: impl IntoIterator<Item = BlockHeight>,
        local_blocks: impl IntoIterator<Item = u32>,
        starting_height: u32,
    ) -> (
        SyncTask,
        StateWatcher,
        tokio::sync::watch::Sender<fuel_core_services::State>,
    ) {
        let connections_stream = MockStream::new(connections_stream).into_boxed();
        let peer_height_stream = MockStream::new(peer_heights).into_boxed();
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
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            1,
            vec![1],
            vec![BlockHeight::from(10u32)],
            vec![10],
            9,
        );

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
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            1,
            vec![1],
            vec![BlockHeight::from(12u32)],
            vec![],
            10,
        );

        let _ = sync_task.run(&mut watcher).await; // peers
        let _ = sync_task.run(&mut watcher).await; // peer height

        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
    }

    #[tokio::test]
    async fn sync_task__falls_out_of_sync_when_peers_pull_ahead() {
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            1,
            vec![1],
            vec![BlockHeight::from(10u32), BlockHeight::from(13u32)],
            vec![],
            10,
        );

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
        let (mut sync_task, mut watcher, _tx) = configure_sync_task(
            2,
            vec![1],
            vec![BlockHeight::from(10u32)],
            vec![],
            10,
        );

        let _ = sync_task.run(&mut watcher).await;
        let _ = sync_task.run(&mut watcher).await;

        assert_eq!(SyncState::NotSynced, *sync_task.state_receiver.borrow());
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
