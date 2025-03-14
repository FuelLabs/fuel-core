use crate::global_registry;
use prometheus_client::metrics::gauge::Gauge;
use std::sync::OnceLock;

pub struct TxStatusManagerMetrics {
    /// Number of prunable statuses in the manager
    pub prunable_status_count: Gauge,
    /// Number of non-prunable statuses in the manager
    pub non_prunable_status_count: Gauge,
    /// The length of the pruning queue
    pub pruning_queue_len: Gauge,
    /// The age of the oldest status in the pruning queue
    pub pruning_queue_oldest_status_age_s: Gauge,
}

impl Default for TxStatusManagerMetrics {
    fn default() -> Self {
        let prunable_status_count = Gauge::default();
        let non_prunable_status_count = Gauge::default();
        let pruning_queue_len = Gauge::default();
        let pruning_queue_oldest_status_age_s = Gauge::default();

        let metrics = TxStatusManagerMetrics {
            prunable_status_count,
            non_prunable_status_count,
            pruning_queue_len,
            pruning_queue_oldest_status_age_s,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "tx_status_manager_prunable_status_count",
            "The total number of prunable statuses currently stored in the tx status manager",
            metrics.prunable_status_count.clone(),
        );
        registry.register(
            "tx_status_manager_non_prunable_status_count",
            "The total number of non-prunable statuses currently stored in the tx status manager",
            metrics.non_prunable_status_count.clone(),
        );
        registry.register(
            "tx_status_manager_pruning_queue_len",
            "The number of entries currently stored in the pruning queue",
            metrics.pruning_queue_len.clone(),
        );
        registry.register(
            "tx_status_manager_pruning_queue_oldest_status_age_s",
            "The age of the oldest transaction in the pruning queue",
            metrics.pruning_queue_oldest_status_age_s.clone(),
        );
        metrics
    }
}

static TX_STATUS_MANAGER_METRICS: OnceLock<TxStatusManagerMetrics> = OnceLock::new();
pub fn metrics_manager() -> &'static TxStatusManagerMetrics {
    TX_STATUS_MANAGER_METRICS.get_or_init(TxStatusManagerMetrics::default)
}
