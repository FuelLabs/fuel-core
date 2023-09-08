use crate::timing_buckets;
use prometheus_client::{
    metrics::{
        gauge::Gauge,
        histogram::Histogram,
    },
    registry::Registry,
};
use std::sync::{
    atomic::AtomicU64,
    OnceLock,
};

pub struct ImporterMetrics {
    pub registry: Registry,
    // using gauges in case blocks are rolled back for any reason
    pub total_txs_count: Gauge,
    pub block_height: Gauge,
    pub latest_block_import_timestamp: Gauge<f64, AtomicU64>,
    pub execute_and_commit_duration: Histogram,
}

impl Default for ImporterMetrics {
    fn default() -> Self {
        let mut registry = Registry::default();

        let tx_count_gauge = Gauge::default();
        let block_height_gauge = Gauge::default();
        let latest_block_import_ms = Gauge::default();
        let execute_and_commit_duration =
            Histogram::new(timing_buckets().iter().cloned());

        registry.register(
            "importer_tx_count",
            "the total amount of transactions that have been imported on chain",
            tx_count_gauge.clone(),
        );

        registry.register(
            "importer_block_height",
            "the current height of the chain",
            block_height_gauge.clone(),
        );

        registry.register(
            "importer_latest_block_commit_timestamp_s",
            "A timestamp of when the current block was imported",
            latest_block_import_ms.clone(),
        );

        registry.register(
            "importer_execute_and_commit_duration_s",
            "Records the duration time of executing and committing a block",
            execute_and_commit_duration.clone(),
        );

        Self {
            registry,
            total_txs_count: tx_count_gauge,
            block_height: block_height_gauge,
            latest_block_import_timestamp: latest_block_import_ms,
            execute_and_commit_duration,
        }
    }
}

// Setup a global static for accessing importer metrics
static IMPORTER_METRICS: OnceLock<ImporterMetrics> = OnceLock::new();

pub fn importer_metrics() -> &'static ImporterMetrics {
    IMPORTER_METRICS.get_or_init(ImporterMetrics::default)
}
