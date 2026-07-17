use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use prometheus_client::metrics::{
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::OnceLock;

/// Metrics for the block-production execution stage, recorded around the
/// executor's `produce_without_commit` call. Unlike the importer metrics,
/// these fire on the producer (leader) path for both the sequential and the
/// parallel executor, so block execution time is observable regardless of
/// the executor mode.
pub struct ProducerMetrics {
    /// Wall time of executing a produced block (seconds).
    pub block_execution_duration: Histogram,
    /// Wall time of executing the last produced block (milliseconds).
    pub last_block_execution_time_ms: Gauge,
    /// Actual gas used by the transactions of the last produced block.
    pub last_block_gas_used: Gauge,
    /// Number of transactions included in the last produced block.
    pub last_block_transactions: Gauge,
}

impl Default for ProducerMetrics {
    fn default() -> Self {
        let block_execution_duration = Histogram::new(buckets(Buckets::Timing));
        let last_block_execution_time_ms = Gauge::default();
        let last_block_gas_used = Gauge::default();
        let last_block_transactions = Gauge::default();

        let mut registry = global_registry().registry.lock();
        registry.register(
            "producer_block_execution_duration_s",
            "Wall time of executing a produced block (production path, any executor mode)",
            block_execution_duration.clone(),
        );

        registry.register(
            "producer_last_block_execution_time_ms",
            "Wall time of executing the last produced block in milliseconds",
            last_block_execution_time_ms.clone(),
        );

        registry.register(
            "producer_last_block_gas_used",
            "Actual gas used by the transactions of the last produced block",
            last_block_gas_used.clone(),
        );

        registry.register(
            "producer_last_block_transactions",
            "Number of transactions included in the last produced block",
            last_block_transactions.clone(),
        );

        Self {
            block_execution_duration,
            last_block_execution_time_ms,
            last_block_gas_used,
            last_block_transactions,
        }
    }
}

static PRODUCER_METRICS: OnceLock<ProducerMetrics> = OnceLock::new();

pub fn producer_metrics() -> &'static ProducerMetrics {
    PRODUCER_METRICS.get_or_init(ProducerMetrics::default)
}
