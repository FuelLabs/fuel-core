use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use fuel_core_poa::ports::BlockImporter;
use fuel_core_storage::transactional::HistoricalView;
use futures::StreamExt;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, warn};

use crate::{
    cluster::Cluster,
    timeline::{Timeline, TimelineEventKind, Violation},
};

/// A block event received from a node's block stream.
pub struct BlockEvent {
    node_idx: usize,
    height: u32,
    block_id: String,
    is_local: bool,
}

/// State maintained by the invariant checker.
struct InvariantState {
    /// height -> (block_id, first_node_that_saw_it)
    canonical: HashMap<u32, (String, usize)>,
    /// height -> Vec<(node_idx, timestamp)> for locally produced blocks
    local_producers: HashMap<u32, Vec<(usize, Instant)>>,
}

impl InvariantState {
    fn new() -> Self {
        Self {
            canonical: HashMap::new(),
            local_producers: HashMap::new(),
        }
    }

    fn check_block(
        &mut self,
        event: &BlockEvent,
        timeline: &Timeline,
        lease_ttl: Duration,
    ) {
        let height = event.height;
        let node_idx = event.node_idx;

        // Record block in timeline
        timeline.record(TimelineEventKind::BlockProduced {
            node: node_idx,
            height,
            local: event.is_local,
        });

        // 1. Fork detection
        if let Some((existing_id, first_node)) =
            self.canonical.get(&height)
        {
            if *existing_id != event.block_id {
                let violation = Violation::Fork {
                    height,
                    first_block_id: existing_id.clone(),
                    second_block_id: event.block_id.clone(),
                    first_node: *first_node,
                    second_node: node_idx,
                };
                warn!("INVARIANT VIOLATION: {violation}");
                timeline.record(TimelineEventKind::Violation(violation));
            }
        } else {
            self.canonical
                .insert(height, (event.block_id.clone(), node_idx));
        }

        // 2. Concurrent leader detection
        if event.is_local {
            let producers = self
                .local_producers
                .entry(height)
                .or_insert_with(Vec::new);
            producers.push((node_idx, Instant::now()));

            if producers.len() > 1 {
                let first_time = producers[0].1;
                let last_time = producers.last().unwrap().1;
                let gap = last_time.duration_since(first_time);

                // Only flag if within lease_ttl (brief overlap during handoff is expected)
                if gap < lease_ttl {
                    let nodes: Vec<usize> =
                        producers.iter().map(|(n, _)| *n).collect();
                    let violation =
                        Violation::ConcurrentLeaders { height, nodes };
                    warn!("INVARIANT VIOLATION: {violation}");
                    timeline
                        .record(TimelineEventKind::Violation(violation));
                }
            }
        }
    }
}

/// Poll the actual committed database height for each live node.
/// This is reliable — it reads directly from RocksDB, not from the lossy
/// broadcast stream which silently drops events when the consumer lags.
fn check_gaps_from_db(
    cluster: &Cluster,
    gap_tolerance: Duration,
    last_progress: &mut HashMap<usize, (u32, Instant)>,
    timeline: &Timeline,
) {
    let mut max_height: u32 = 0;
    let mut node_heights: Vec<(usize, u32)> = Vec::new();

    for (idx, handle) in cluster.live_nodes() {
        let height = handle
            .service
            .shared
            .database
            .on_chain()
            .latest_height()
            .map(|h| u32::from(h))
            .unwrap_or(0);
        node_heights.push((idx, height));
        if height > max_height {
            max_height = height;
        }
    }

    if max_height == 0 {
        return;
    }

    for (node_idx, height) in &node_heights {
        let entry = last_progress
            .entry(*node_idx)
            .or_insert((*height, Instant::now()));

        // Update timestamp if height advanced
        if *height > entry.0 {
            *entry = (*height, Instant::now());
        }

        let behind = max_height.saturating_sub(*height);
        if behind > 10 {
            let stalled_for = entry.1.elapsed();
            if stalled_for > gap_tolerance {
                let violation = Violation::GapDetected {
                    node: *node_idx,
                    max_global_height: max_height,
                    node_height: *height,
                    behind_for: stalled_for,
                };
                warn!("INVARIANT VIOLATION: {violation}");
                timeline
                    .record(TimelineEventKind::Violation(violation));
            }
        }
    }
}

/// Spawns block stream reader tasks for all live nodes, feeding events
/// into the invariant checker via an mpsc channel.
pub fn spawn_block_readers(
    cluster: &Cluster,
    tx: mpsc::UnboundedSender<BlockEvent>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    for (node_idx, node) in cluster.live_nodes() {
        let sender = tx.clone();
        let mut stream = node.service.shared.block_importer.block_stream();

        let handle = tokio::spawn(async move {
            while let Some(block) = stream.next().await {
                let height = u32::from(*block.block_header.height());
                let block_id = format!("{:?}", block.block_header.id());
                let is_local = block.is_locally_produced();

                let event = BlockEvent {
                    node_idx,
                    height,
                    block_id,
                    is_local,
                };

                if sender.send(event).is_err() {
                    break;
                }
            }
            debug!("Block reader for node {node_idx} ended");
        });

        handles.push(handle);
    }

    handles
}

/// Runs the invariant checker loop. Receives block events and checks invariants.
pub async fn run_invariant_checker(
    cluster: Arc<Mutex<Cluster>>,
    timeline: Timeline,
    mut stop: tokio::sync::watch::Receiver<bool>,
    lease_ttl: Duration,
    gap_tolerance: Duration,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<BlockEvent>();
    let mut state = InvariantState::new();
    let mut reader_handles: Vec<tokio::task::JoinHandle<()>>;

    // Spawn initial block readers
    {
        let cluster_guard = cluster.lock().await;
        reader_handles = spawn_block_readers(&cluster_guard, tx.clone());
    }

    let mut gap_check_interval = tokio::time::interval(Duration::from_secs(10));
    // Track last-progress per node for DB-based gap detection
    let mut last_progress: HashMap<usize, (u32, Instant)> = HashMap::new();

    // Track which nodes had readers spawned
    let mut reader_nodes: Vec<bool> = {
        let cluster_guard = cluster.lock().await;
        (0..cluster_guard.node_count())
            .map(|i| cluster_guard.is_node_alive(i))
            .collect()
    };

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                state.check_block(&event, &timeline, lease_ttl);
            }
            _ = gap_check_interval.tick() => {
                let cluster_guard = cluster.lock().await;

                // Gap detection via direct DB polling (reliable)
                check_gaps_from_db(
                    &cluster_guard,
                    gap_tolerance,
                    &mut last_progress,
                    &timeline,
                );

                // Spawn readers for newly alive nodes
                let live_status: Vec<(usize, bool)> = (0..cluster_guard.node_count())
                    .map(|i| (i, cluster_guard.is_node_alive(i)))
                    .collect();
                for (idx, is_alive) in &live_status {
                    if *is_alive && !reader_nodes.get(*idx).copied().unwrap_or(false) {
                        info!("Spawning new block reader for restarted node {idx}");
                        let sender = tx.clone();
                        if let Some((_, node)) = cluster_guard.live_nodes().into_iter().find(|(i, _)| i == idx) {
                            let node_idx = *idx;
                            let mut stream = node.service.shared.block_importer.block_stream();
                            let handle = tokio::spawn(async move {
                                while let Some(block) = stream.next().await {
                                    let height = u32::from(*block.block_header.height());
                                    let block_id = format!("{:?}", block.block_header.id());
                                    let is_local = block.is_locally_produced();

                                    let event = BlockEvent {
                                        node_idx,
                                        height,
                                        block_id,
                                        is_local,
                                    };
                                    if sender.send(event).is_err() {
                                        break;
                                    }
                                }
                                debug!("Block reader for node {node_idx} ended");
                            });
                            reader_handles.push(handle);
                        }
                    }
                }

                // Update reader tracking
                reader_nodes = live_status.iter().map(|(_, alive)| *alive).collect();
            }
            _ = stop.changed() => {
                if *stop.borrow() {
                    info!("Invariant checker stopping");
                    break;
                }
            }
        }
    }

    // Drain remaining events
    while let Ok(event) = rx.try_recv() {
        state.check_block(&event, &timeline, lease_ttl);
    }

    // Abort reader tasks
    for handle in reader_handles {
        handle.abort();
    }
}
