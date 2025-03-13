use crate::global_registry;
use prometheus_client::metrics::gauge::Gauge;
use std::sync::OnceLock;

pub struct TxStatusManagerMetrics {
    /// Number of prunable statuses in the manager
    pub number_of_prunable_statuses: Gauge,
    /// Number of non-prunable statuses in the manager
    pub number_of_non_prunable_statuses: Gauge,
    /// The length of the pruning queue
    pub pruning_queue_length: Gauge,
    /// The age of the oldest status in the pruning queue
    pub pruning_queue_oldest_status_age_seconds: Gauge,
}

impl Default for TxStatusManagerMetrics {
    fn default() -> Self {
        let number_of_prunable_statuses = Gauge::default();
        let number_of_non_prunable_statuses = Gauge::default();
        let pruning_queue_length = Gauge::default();
        let pruning_queue_oldest_status_age_seconds = Gauge::default();

        let metrics = TxStatusManagerMetrics {
            number_of_prunable_statuses,
            number_of_non_prunable_statuses,
            pruning_queue_length,
            pruning_queue_oldest_status_age_seconds,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "tx_status_manager_number_of_prunable_statuses",
            "The total number of prunable statuses currently stored in the tx status manager",
            metrics.number_of_prunable_statuses.clone(),
        );
        registry.register(
            "tx_status_manager_number_of_non_prunable_statuses",
            "The total number of non-prunable statuses currently stored in the tx status manager",
            metrics.number_of_non_prunable_statuses.clone(),
        );
        registry.register(
            "tx_status_manager_pruning_queue_length",
            "The number of entries currently stored in the pruning queue",
            metrics.pruning_queue_length.clone(),
        );
        registry.register(
            "tx_status_manager_pruning_queue_oldest_status_age_seconds",
            "The age of the oldest transaction in the pruning queue",
            metrics.pruning_queue_oldest_status_age_seconds.clone(),
        );
        metrics
    }
}

static TX_STATUS_MANAGER_METRICS: OnceLock<TxStatusManagerMetrics> = OnceLock::new();
pub fn tx_status_manager_metrics() -> &'static TxStatusManagerMetrics {
    TX_STATUS_MANAGER_METRICS.get_or_init(TxStatusManagerMetrics::default)
}
