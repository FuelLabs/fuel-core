use std::{
    collections::VecDeque,
    fmt,
    sync::Arc,
    time::Duration,
};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{
    cluster::Cluster,
    proxy::ProxyMode,
    timeline::{Timeline, TimelineEventKind},
};

#[derive(Debug, Clone)]
pub enum FaultAction {
    KillRedis(usize),
    RestartRedis(usize),
    PartitionNodeFromRedis {
        node_idx: usize,
        redis_idx: usize,
    },
    PartitionAllFromRedis(usize),
    AddLatency {
        node_idx: usize,
        redis_idx: usize,
        ms: u64,
    },
    DropMidOperation {
        node_idx: usize,
        redis_idx: usize,
        after_bytes: usize,
    },
    RestoreProxy {
        node_idx: usize,
        redis_idx: usize,
    },
    RestoreAllProxies,
    KillNode(usize),
    RestartNode(usize),
}

impl fmt::Display for FaultAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaultAction::KillRedis(idx) => write!(f, "Kill Redis {idx}"),
            FaultAction::RestartRedis(idx) => {
                write!(f, "Restart Redis {idx}")
            }
            FaultAction::PartitionNodeFromRedis {
                node_idx,
                redis_idx,
            } => {
                write!(
                    f,
                    "Partition node {node_idx} from Redis {redis_idx}"
                )
            }
            FaultAction::PartitionAllFromRedis(idx) => {
                write!(f, "Partition all nodes from Redis {idx}")
            }
            FaultAction::AddLatency {
                node_idx,
                redis_idx,
                ms,
            } => {
                write!(
                    f,
                    "Add {ms}ms latency: node {node_idx} <-> Redis {redis_idx}"
                )
            }
            FaultAction::DropMidOperation {
                node_idx,
                redis_idx,
                after_bytes,
            } => {
                write!(
                    f,
                    "Drop after {after_bytes}B: node {node_idx} <-> Redis {redis_idx}"
                )
            }
            FaultAction::RestoreProxy {
                node_idx,
                redis_idx,
            } => {
                write!(
                    f,
                    "Restore proxy: node {node_idx} <-> Redis {redis_idx}"
                )
            }
            FaultAction::RestoreAllProxies => write!(f, "Restore all proxies"),
            FaultAction::KillNode(idx) => write!(f, "Kill node {idx}"),
            FaultAction::RestartNode(idx) => write!(f, "Restart node {idx}"),
        }
    }
}

struct PendingRecovery {
    action: FaultAction,
    at: tokio::time::Instant,
}

pub struct FaultScheduler {
    rng: StdRng,
    fault_interval: Duration,
    node_count: usize,
    redis_count: usize,
    pending_recoveries: VecDeque<PendingRecovery>,
}

impl FaultScheduler {
    pub fn new(
        seed: u64,
        fault_interval: Duration,
        node_count: usize,
        redis_count: usize,
    ) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            fault_interval,
            node_count,
            redis_count,
            pending_recoveries: VecDeque::new(),
        }
    }

    pub async fn run(
        mut self,
        cluster: Arc<Mutex<Cluster>>,
        timeline: Timeline,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            // Check for stop signal
            if *stop.borrow() {
                return;
            }

            // Process any pending recoveries that are due
            self.process_recoveries(&cluster, &timeline).await;

            // Calculate jittered interval: ±50%
            let jitter_factor = 0.5 + self.rng.r#gen::<f64>();
            let interval = Duration::from_secs_f64(
                self.fault_interval.as_secs_f64() * jitter_factor,
            );

            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = stop.changed() => {
                    if *stop.borrow() {
                        return;
                    }
                }
            }

            // Pick and apply a fault
            let cluster_guard = cluster.lock().await;
            let action = self.pick_fault(&cluster_guard);
            drop(cluster_guard);

            if let Some(action) = action {
                self.apply_fault(&cluster, &timeline, action).await;
            }
        }
    }

    async fn process_recoveries(
        &mut self,
        cluster: &Arc<Mutex<Cluster>>,
        timeline: &Timeline,
    ) {
        let now = tokio::time::Instant::now();
        while self
            .pending_recoveries
            .front()
            .is_some_and(|r| r.at <= now)
        {
            let recovery = self.pending_recoveries.pop_front().unwrap();
            info!("Applying scheduled recovery: {}", recovery.action);
            timeline.record(TimelineEventKind::FaultRecovered(
                recovery.action.clone(),
            ));
            let mut cluster_guard = cluster.lock().await;
            self.apply_action(&mut cluster_guard, &recovery.action).await;
        }
    }

    fn pick_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        // Weighted random selection
        let roll: f64 = self.rng.r#gen();

        if roll < 0.25 {
            // Network partition (25%)
            self.pick_partition_fault(cluster)
        } else if roll < 0.45 {
            // Latency (20%)
            self.pick_latency_fault(cluster)
        } else if roll < 0.60 {
            // Mid-operation drop (15%)
            self.pick_mid_op_fault(cluster)
        } else if roll < 0.75 {
            // Redis kill/restart (15%)
            self.pick_redis_fault(cluster)
        } else if roll < 0.90 {
            // Node kill/restart (15%)
            self.pick_node_fault(cluster)
        } else if roll < 0.95 {
            // Restore single proxy (5%)
            self.pick_restore_single(cluster)
        } else {
            // Restore all (5%)
            Some(FaultAction::RestoreAllProxies)
        }
    }

    fn pick_partition_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        if self.rng.gen_bool(0.3) {
            // Partition all nodes from one Redis
            let redis_idx = self.rng.gen_range(0..self.redis_count);
            Some(FaultAction::PartitionAllFromRedis(redis_idx))
        } else {
            // Partition one node from one Redis
            let node_idx = self.rng.gen_range(0..self.node_count);
            let redis_idx = self.rng.gen_range(0..self.redis_count);
            if !cluster.is_node_alive(node_idx) {
                return None;
            }
            Some(FaultAction::PartitionNodeFromRedis {
                node_idx,
                redis_idx,
            })
        }
    }

    fn pick_latency_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        let node_idx = self.rng.gen_range(0..self.node_count);
        let redis_idx = self.rng.gen_range(0..self.redis_count);
        if !cluster.is_node_alive(node_idx) {
            return None;
        }
        let ms = self.rng.gen_range(50..500);
        Some(FaultAction::AddLatency {
            node_idx,
            redis_idx,
            ms,
        })
    }

    fn pick_mid_op_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        let node_idx = self.rng.gen_range(0..self.node_count);
        let redis_idx = self.rng.gen_range(0..self.redis_count);
        if !cluster.is_node_alive(node_idx) {
            return None;
        }
        let after_bytes = self.rng.gen_range(10..500);
        Some(FaultAction::DropMidOperation {
            node_idx,
            redis_idx,
            after_bytes,
        })
    }

    fn pick_redis_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        let redis_idx = self.rng.gen_range(0..self.redis_count);

        if cluster.is_redis_alive(redis_idx) {
            // Check safety: don't kill more than redis_count - quorum
            let alive_redis = cluster.alive_redis_count();
            let quorum = self.redis_count / 2 + 1;
            if alive_redis <= quorum {
                warn!(
                    "Skipping Redis kill: only {alive_redis} alive, need {quorum} for quorum"
                );
                return None;
            }
            Some(FaultAction::KillRedis(redis_idx))
        } else {
            Some(FaultAction::RestartRedis(redis_idx))
        }
    }

    fn pick_node_fault(&mut self, cluster: &Cluster) -> Option<FaultAction> {
        let node_idx = self.rng.gen_range(0..self.node_count);

        if cluster.is_node_alive(node_idx) {
            // Don't kill the last alive node
            let alive_nodes = cluster.alive_node_count();
            if alive_nodes <= 1 {
                warn!("Skipping node kill: only 1 node alive");
                return None;
            }
            Some(FaultAction::KillNode(node_idx))
        } else {
            Some(FaultAction::RestartNode(node_idx))
        }
    }

    fn pick_restore_single(&mut self, _cluster: &Cluster) -> Option<FaultAction> {
        let node_idx = self.rng.gen_range(0..self.node_count);
        let redis_idx = self.rng.gen_range(0..self.redis_count);
        Some(FaultAction::RestoreProxy {
            node_idx,
            redis_idx,
        })
    }

    async fn apply_fault(
        &mut self,
        cluster: &Arc<Mutex<Cluster>>,
        timeline: &Timeline,
        action: FaultAction,
    ) {
        info!("Injecting fault: {action}");
        timeline
            .record(TimelineEventKind::FaultInjected(action.clone()));

        // Schedule recovery for destructive faults
        match &action {
            FaultAction::KillRedis(_)
            | FaultAction::KillNode(_)
            | FaultAction::PartitionNodeFromRedis { .. }
            | FaultAction::PartitionAllFromRedis(_) => {
                let recovery_delay = Duration::from_secs(
                    self.rng.gen_range(3..=10),
                );
                let recovery_action = match &action {
                    FaultAction::KillRedis(idx) => {
                        FaultAction::RestartRedis(*idx)
                    }
                    FaultAction::KillNode(idx) => {
                        FaultAction::RestartNode(*idx)
                    }
                    FaultAction::PartitionNodeFromRedis {
                        node_idx,
                        redis_idx,
                    } => FaultAction::RestoreProxy {
                        node_idx: *node_idx,
                        redis_idx: *redis_idx,
                    },
                    FaultAction::PartitionAllFromRedis(_) => {
                        FaultAction::RestoreAllProxies
                    }
                    _ => unreachable!(),
                };
                self.pending_recoveries.push_back(PendingRecovery {
                    action: recovery_action,
                    at: tokio::time::Instant::now() + recovery_delay,
                });
            }
            _ => {}
        }

        let mut cluster_guard = cluster.lock().await;
        self.apply_action(&mut cluster_guard, &action).await;

        // Safety check: if no live node can reach a Redis quorum after
        // applying this fault, undo it immediately. This prevents the
        // scheduler from creating scenarios where block production is
        // impossible (which would cause expected but uninformative stalls).
        if !cluster_guard.any_node_has_redis_quorum() {
            warn!(
                "Fault '{action}' broke Redis quorum for all nodes — reverting"
            );
            self.undo_action(&mut cluster_guard, &action).await;
            self.pending_recoveries.pop_back(); // remove scheduled recovery
        }
    }

    async fn apply_action(
        &self,
        cluster: &mut Cluster,
        action: &FaultAction,
    ) {
        match action {
            FaultAction::KillRedis(idx) => {
                cluster.stop_redis(*idx);
            }
            FaultAction::RestartRedis(idx) => {
                cluster.start_redis(*idx);
            }
            FaultAction::PartitionNodeFromRedis {
                node_idx,
                redis_idx,
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::DropAll,
                );
            }
            FaultAction::PartitionAllFromRedis(redis_idx) => {
                for node_idx in 0..self.node_count {
                    cluster.set_proxy_mode(
                        node_idx,
                        *redis_idx,
                        ProxyMode::DropAll,
                    );
                }
            }
            FaultAction::AddLatency {
                node_idx,
                redis_idx,
                ms,
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::Latency(Duration::from_millis(*ms)),
                );
            }
            FaultAction::DropMidOperation {
                node_idx,
                redis_idx,
                after_bytes,
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::CloseAfterBytes(*after_bytes),
                );
            }
            FaultAction::RestoreProxy {
                node_idx,
                redis_idx,
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::Normal,
                );
            }
            FaultAction::RestoreAllProxies => {
                cluster.restore_all_proxies();
            }
            FaultAction::KillNode(idx) => {
                cluster.stop_node(*idx).await;
            }
            FaultAction::RestartNode(idx) => {
                cluster.restart_node(*idx).await;
            }
        }
    }

    /// Undo a fault action (best-effort inverse).
    async fn undo_action(
        &self,
        cluster: &mut Cluster,
        action: &FaultAction,
    ) {
        match action {
            FaultAction::KillRedis(idx) => {
                cluster.start_redis(*idx);
            }
            FaultAction::PartitionNodeFromRedis {
                node_idx,
                redis_idx,
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::Normal,
                );
            }
            FaultAction::PartitionAllFromRedis(redis_idx) => {
                for node_idx in 0..self.node_count {
                    cluster.set_proxy_mode(
                        node_idx,
                        *redis_idx,
                        ProxyMode::Normal,
                    );
                }
            }
            FaultAction::AddLatency {
                node_idx,
                redis_idx,
                ..
            }
            | FaultAction::DropMidOperation {
                node_idx,
                redis_idx,
                ..
            } => {
                cluster.set_proxy_mode(
                    *node_idx,
                    *redis_idx,
                    ProxyMode::Normal,
                );
            }
            FaultAction::KillNode(idx) => {
                cluster.restart_node(*idx).await;
            }
            // Recovery actions don't need undoing
            _ => {}
        }
    }
}
