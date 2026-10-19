use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::Histogram,
    },
};
use std::{
    sync::{
        OnceLock,
        atomic::{
            AtomicU64,
            Ordering,
        },
    },
    time::{
        Duration,
        SystemTime,
        UNIX_EPOCH,
    },
};

/// A quorum fan-out operation: a point where the adapter contacts all N
/// Redis nodes in parallel and waits for a quorum to respond. Used as
/// the `operation` label on `poa_quorum_latency_s`.
#[derive(Clone, Copy, Debug)]
pub enum QuorumOp {
    /// `publish_block_on_all_nodes`: write_block.lua fan-out, until a
    /// quorum of nodes return `Written`.
    Publish,
    /// `has_lease_owner_quorum`: lease-owner check fan-out, until a
    /// quorum of nodes confirm this authority owns the lock.
    OwnerCheck,
    /// `should_reconcile_from_stream`: latest-entry read fan-out, until
    /// a quorum of nodes have responded.
    ReconcileRead,
    /// `acquire_lease_if_free`: a single `promote_leader` fan-out, until
    /// a quorum of nodes grant the lease.
    Promote,
}

impl QuorumOp {
    fn as_label(self) -> &'static str {
        match self {
            QuorumOp::Publish => "publish",
            QuorumOp::OwnerCheck => "owner_check",
            QuorumOp::ReconcileRead => "reconcile_read",
            QuorumOp::Promote => "promote",
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct QuorumOpLabel {
    operation: String,
}

pub struct PoAMetrics {
    /// Current fencing token epoch value.
    pub leader_epoch: Gauge,
    /// 1 while the last successful lease acquisition or quorum renewal
    /// is still inside the lease TTL, 0 after that TTL passes or the
    /// lease is released. Recomputed on each metrics scrape so a wedged
    /// PoA task cannot leave the gauge stuck at 1.
    pub is_leader: Gauge,
    /// Unix timestamp of the last block this node published to a Redis
    /// quorum. Stays at the last value so scrapers can compute age with
    /// `time() - poa_last_produced_block_timestamp_seconds`. 0 if this
    /// process has not published a block.
    pub last_produced_block_timestamp_seconds: Gauge,
    /// Unix timestamp of the last successful lease acquisition or quorum
    /// `write_block` renewal. 0 if this process has not held the lease.
    pub lease_last_renewed_timestamp_seconds: Gauge,
    /// Unix millis when the current lease stops counting as held.
    /// 0 means this process does not hold a live lease.
    lease_valid_until_unix_millis: AtomicU64,
    /// Seqlock for `lease_valid_until_unix_millis` and `is_leader`.
    /// Even: stable. Odd: a renewal or release is writing. A scrape
    /// retries until it observes an even generation that does not
    /// change across the gauge update, so it cannot stick `is_leader`
    /// at 0 after a renewal or at 1 after a release.
    lease_generation: AtomicU64,
    /// Maximum epoch difference across Redis quorum nodes
    /// observed during promotion.
    pub epoch_max_drift: Gauge,
    /// (Min height in Redis stream) - (max committed local height).
    /// Large positive = stream has old blocks not yet trimmed.
    /// Near zero = XTRIM may remove blocks the follower hasn't synced.
    pub stream_trim_headroom: Gauge,
    /// Background per-node publish tasks currently in flight (incremented
    /// when the task is spawned, decremented when its closure exits).
    /// Steady-state should be near zero; sustained values into the
    /// hundreds indicate per-node publishes are not exiting and should
    /// be investigated (likely a regression in the per-call timeout or
    /// a connection/protocol stall not covered by `node_timeout`).
    pub outstanding_publish_tasks: Gauge,
    /// Successful write_block.lua per-node invocations.
    pub write_block_success_total: Counter,
    /// write_block.lua rejections due to HEIGHT_EXISTS.
    pub write_block_height_exists_total: Counter,
    /// write_block.lua rejections due to FENCING_ERROR.
    pub write_block_fencing_error_total: Counter,
    /// write_block.lua failures from connection or other errors.
    pub write_block_error_total: Counter,

    /// Successful sub-quorum block repairs.
    pub repair_success_total: Counter,
    /// Failed sub-quorum block repairs (quorum not reached).
    pub repair_failure_total: Counter,

    /// Successful lock promotions (lease acquired).
    pub promotion_success_total: Counter,
    /// Lock promotions where a quorum of nodes reported the lock held
    /// by another authority — the normal follower-defer path, not a
    /// failure. Tracked separately from `promotion_failure_total` so
    /// dashboards can see follower-cycle cadence without that signal
    /// drowning out true failures.
    pub promotion_lock_held_total: Counter,
    /// Lock promotions that exhausted all retry attempts without
    /// either acquiring the lease or determining via quorum that the
    /// lock is held — i.e. the cluster could not reach a verdict.
    /// Does not include follower defers (see
    /// `promotion_lock_held_total`).
    pub promotion_failure_total: Counter,

    /// Redis connection cache clears (connection errors/timeouts).
    pub connection_reset_total: Counter,

    /// Time to complete lease acquisition (acquire_lease_if_free).
    pub promotion_duration_s: Histogram,
    /// Time per write_block.lua invocation including network RTT.
    pub write_block_duration_s: Histogram,
    /// Time to complete stream reconciliation.
    pub reconciliation_duration_s: Histogram,

    /// Wall-clock time from a quorum fan-out's start until a quorum of
    /// Redis nodes responded successfully, labeled by `operation` (see
    /// [`QuorumOp`]). Recorded *only* when quorum is reached — this is
    /// "time to quorum", the latency the caller actually waits for,
    /// unlike `write_block_duration_s` which is per-node RTT and so
    /// hides the fan-out's quorum-completion behind its slowest node.
    quorum_latency_s: Family<QuorumOpLabel, Histogram>,
}

impl Default for PoAMetrics {
    fn default() -> Self {
        let leader_epoch = Gauge::default();
        let is_leader = Gauge::default();
        let last_produced_block_timestamp_seconds = Gauge::default();
        let lease_last_renewed_timestamp_seconds = Gauge::default();
        let epoch_max_drift = Gauge::default();
        let stream_trim_headroom = Gauge::default();
        let outstanding_publish_tasks = Gauge::default();
        let write_block_success_total = Counter::default();
        let write_block_height_exists_total = Counter::default();
        let write_block_fencing_error_total = Counter::default();
        let write_block_error_total = Counter::default();

        let repair_success_total = Counter::default();
        let repair_failure_total = Counter::default();

        let promotion_success_total = Counter::default();
        let promotion_lock_held_total = Counter::default();
        let promotion_failure_total = Counter::default();

        let connection_reset_total = Counter::default();

        let promotion_duration_s = Histogram::new(buckets(Buckets::Timing));
        let write_block_duration_s = Histogram::new(buckets(Buckets::Timing));
        let reconciliation_duration_s = Histogram::new(buckets(Buckets::Timing));
        let quorum_latency_s =
            Family::<QuorumOpLabel, Histogram>::new_with_constructor(|| {
                Histogram::new(buckets(Buckets::Timing))
            });

        let mut registry = global_registry().registry.lock();

        registry.register(
            "poa_leader_epoch",
            "Current fencing token epoch value",
            leader_epoch.clone(),
        );
        registry.register(
            "poa_is_leader",
            "1 while the last successful lease acquisition or quorum renewal is still inside the lease TTL, 0 once that TTL passes without a renewal or the lease is released",
            is_leader.clone(),
        );
        registry.register(
            "poa_last_produced_block_timestamp_seconds",
            "Unix timestamp of the last block this node published to a Redis quorum",
            last_produced_block_timestamp_seconds.clone(),
        );
        registry.register(
            "poa_lease_last_renewed_timestamp_seconds",
            "Unix timestamp of the last successful lease acquisition or quorum write_block renewal",
            lease_last_renewed_timestamp_seconds.clone(),
        );
        registry.register(
            "poa_epoch_max_drift",
            "Maximum epoch difference across Redis quorum nodes during promotion",
            epoch_max_drift.clone(),
        );
        registry.register(
            "poa_stream_trim_headroom",
            "Min height in Redis stream minus max committed local height",
            stream_trim_headroom.clone(),
        );
        registry.register(
            "poa_outstanding_publish_tasks",
            "Background per-node publish tasks currently in flight",
            outstanding_publish_tasks.clone(),
        );
        // Counter names omit `_total` — prometheus-client adds it automatically.
        registry.register(
            "poa_write_block_success",
            "Successful write_block.lua per-node invocations",
            write_block_success_total.clone(),
        );
        registry.register(
            "poa_write_block_height_exists",
            "write_block.lua rejections due to HEIGHT_EXISTS",
            write_block_height_exists_total.clone(),
        );
        registry.register(
            "poa_write_block_fencing_error",
            "write_block.lua rejections due to FENCING_ERROR",
            write_block_fencing_error_total.clone(),
        );
        registry.register(
            "poa_write_block_error",
            "write_block.lua failures from connection or other errors",
            write_block_error_total.clone(),
        );

        registry.register(
            "poa_repair_success",
            "Successful sub-quorum block repairs",
            repair_success_total.clone(),
        );
        registry.register(
            "poa_repair_failure",
            "Failed sub-quorum block repairs",
            repair_failure_total.clone(),
        );

        registry.register(
            "poa_promotion_success",
            "Successful lock promotions",
            promotion_success_total.clone(),
        );
        registry.register(
            "poa_promotion_lock_held",
            "Lock promotions where a quorum reported the lock held by \
             another authority (follower-defer; not a failure)",
            promotion_lock_held_total.clone(),
        );
        registry.register(
            "poa_promotion_failure",
            "Lock promotions that exhausted retries without acquiring \
             or determining the lock state (does not include follower \
             defers)",
            promotion_failure_total.clone(),
        );

        registry.register(
            "poa_connection_reset",
            "Redis connection cache clears from errors or timeouts",
            connection_reset_total.clone(),
        );

        registry.register(
            "poa_promotion_duration_s",
            "Time to complete lease acquisition",
            promotion_duration_s.clone(),
        );
        registry.register(
            "poa_write_block_duration_s",
            "Time per write_block.lua invocation including network RTT",
            write_block_duration_s.clone(),
        );
        registry.register(
            "poa_reconciliation_duration_s",
            "Time to complete stream reconciliation",
            reconciliation_duration_s.clone(),
        );
        registry.register(
            "poa_quorum_latency_s",
            "Time from a quorum fan-out's start until a quorum of Redis \
             nodes responded successfully, labeled by operation",
            quorum_latency_s.clone(),
        );

        Self {
            leader_epoch,
            is_leader,
            last_produced_block_timestamp_seconds,
            lease_last_renewed_timestamp_seconds,
            lease_valid_until_unix_millis: AtomicU64::new(0),
            lease_generation: AtomicU64::new(0),
            epoch_max_drift,
            stream_trim_headroom,
            outstanding_publish_tasks,
            write_block_success_total,
            write_block_height_exists_total,
            write_block_fencing_error_total,
            write_block_error_total,
            repair_success_total,
            repair_failure_total,
            promotion_success_total,
            promotion_lock_held_total,
            promotion_failure_total,
            connection_reset_total,
            promotion_duration_s,
            write_block_duration_s,
            reconciliation_duration_s,
            quorum_latency_s,
        }
    }
}

impl PoAMetrics {
    /// Record the time-to-quorum for a quorum fan-out operation. Call
    /// this at the moment quorum is reached, with the elapsed time since
    /// the fan-out began. Do not call it when quorum is *not* reached —
    /// failures are tracked by the per-operation `*_failure`/`*_error`
    /// counters, and mixing them in would skew the latency distribution.
    pub fn observe_quorum_latency(&self, op: QuorumOp, seconds: f64) {
        self.quorum_latency_s
            .get_or_create(&QuorumOpLabel {
                operation: op.as_label().to_string(),
            })
            .observe(seconds);
    }

    /// Record a successful lease acquisition or a quorum `write_block`
    /// renewal. Both reset the Redis lease TTL.
    pub fn note_lease_renewed(&self, lease_ttl: Duration) {
        let now_ms = unix_time_millis();
        let ttl_ms = u64::try_from(lease_ttl.as_millis()).unwrap_or(u64::MAX);
        self.write_lease_deadline(now_ms.saturating_add(ttl_ms), 1, Some(now_ms));
    }

    /// This process no longer holds the lease. Timestamp gauges stay at
    /// their last values so scrapers can still compute age.
    pub fn note_lease_lost(&self) {
        self.write_lease_deadline(0, 0, None);
    }

    /// Record a block this node published to a Redis quorum.
    pub fn note_block_produced(&self) {
        self.last_produced_block_timestamp_seconds
            .set(millis_to_unix_seconds(unix_time_millis()));
    }

    /// Set `poa_is_leader` from the last renewal plus TTL. A scrape calls
    /// this so the gauge drops after the TTL even when the PoA task is
    /// not running `leader_state`.
    pub fn sync_is_leader_from_lease(&self) {
        loop {
            let generation = self.lease_generation.load(Ordering::Acquire);
            if generation_is_writing(generation) {
                std::hint::spin_loop();
                continue;
            }
            let until = self.lease_valid_until_unix_millis.load(Ordering::Acquire);
            let generation_after = self.lease_generation.load(Ordering::Acquire);
            if generation != generation_after {
                continue;
            }
            let want = if until > unix_time_millis() { 1 } else { 0 };
            self.is_leader.set(want);
            if self.lease_generation.load(Ordering::Acquire) == generation {
                return;
            }
        }
    }

    fn write_lease_deadline(
        &self,
        until_ms: u64,
        leader: i64,
        renewed_at_ms: Option<u64>,
    ) {
        self.lease_generation.fetch_add(1, Ordering::Release);
        self.lease_valid_until_unix_millis
            .store(until_ms, Ordering::Release);
        if let Some(renewed_at_ms) = renewed_at_ms {
            self.lease_last_renewed_timestamp_seconds
                .set(millis_to_unix_seconds(renewed_at_ms));
        }
        self.is_leader.set(leader);
        self.lease_generation.fetch_add(1, Ordering::Release);
    }
}

fn generation_is_writing(generation: u64) -> bool {
    (generation & 1) == 1
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn millis_to_unix_seconds(millis: u64) -> i64 {
    let secs = millis.checked_div(1000).unwrap_or(0);
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// Recompute `poa_is_leader` when the PoA metrics have been registered.
/// Does not register them — sentries that never start PoA stay free of
/// these series.
pub fn sync_leader_gauge_if_registered() {
    if let Some(metrics) = POA_METRICS.get() {
        metrics.sync_is_leader_from_lease();
    }
}

static POA_METRICS: OnceLock<PoAMetrics> = OnceLock::new();

pub fn poa_metrics() -> &'static PoAMetrics {
    POA_METRICS.get_or_init(PoAMetrics::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sync_is_leader_clears_gauge_after_ttl_without_another_renewal() {
        let metrics = poa_metrics();
        metrics.note_lease_renewed(Duration::from_millis(80));
        assert_eq!(metrics.is_leader.get(), 1);
        assert!(metrics.lease_last_renewed_timestamp_seconds.get() > 0);
        metrics.note_block_produced();
        let produced = metrics.last_produced_block_timestamp_seconds.get();
        assert!(produced > 0);

        tokio::time::sleep(Duration::from_millis(150)).await;
        metrics.sync_is_leader_from_lease();

        assert_eq!(
            metrics.is_leader.get(),
            0,
            "poa_is_leader must drop once the lease TTL passes without a renewal"
        );
        assert!(metrics.lease_last_renewed_timestamp_seconds.get() > 0);
        assert_eq!(
            metrics.last_produced_block_timestamp_seconds.get(),
            produced
        );

        metrics.note_lease_lost();
        assert_eq!(metrics.is_leader.get(), 0);
        assert!(metrics.lease_last_renewed_timestamp_seconds.get() > 0);
    }
}
