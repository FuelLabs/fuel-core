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
use std::sync::OnceLock;

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
    /// 1 if this node is the leader, 0 if follower.
    pub is_leader: Gauge,
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
    /// Failed lock promotions after all retry attempts.
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
            "1 if this node is the leader, 0 if follower",
            is_leader.clone(),
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
            "poa_promotion_failure",
            "Failed lock promotions after all retry attempts",
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
}

static POA_METRICS: OnceLock<PoAMetrics> = OnceLock::new();

pub fn poa_metrics() -> &'static PoAMetrics {
    POA_METRICS.get_or_init(PoAMetrics::default)
}
