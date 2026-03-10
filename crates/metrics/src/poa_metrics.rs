use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use prometheus_client::metrics::{
    counter::Counter,
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::OnceLock;

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
    /// Height difference between P2P gossip tip and Redis stream tip.
    /// Positive = P2P ahead (Redis lagging); negative = Redis ahead.
    /// Set by external callers that have P2P context.
    pub p2p_redis_height_lag: Gauge,

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
}

impl Default for PoAMetrics {
    fn default() -> Self {
        let leader_epoch = Gauge::default();
        let is_leader = Gauge::default();
        let epoch_max_drift = Gauge::default();
        let stream_trim_headroom = Gauge::default();
        let p2p_redis_height_lag = Gauge::default();

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
            "poa_p2p_redis_height_lag",
            "Height difference between P2P gossip tip and Redis stream tip",
            p2p_redis_height_lag.clone(),
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

        Self {
            leader_epoch,
            is_leader,
            epoch_max_drift,
            stream_trim_headroom,
            p2p_redis_height_lag,
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
        }
    }
}

static POA_METRICS: OnceLock<PoAMetrics> = OnceLock::new();

pub fn poa_metrics() -> &'static PoAMetrics {
    POA_METRICS.get_or_init(PoAMetrics::default)
}
