use crate::{
    buckets::{
        buckets,
        Buckets,
    },
    global_registry,
};
use prometheus_client::metrics::{
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::{
    atomic::AtomicU64,
    OnceLock,
};

pub struct ImporterMetrics {
    pub block_height: Gauge,
    pub latest_block_import_timestamp: Gauge<f64, AtomicU64>,
    pub execute_and_commit_duration: Histogram,
    pub gas_per_block: Gauge,
    pub fee_per_block: Gauge,
    pub transactions_per_block: Gauge,
    pub gas_price: Gauge,
}

impl Default for ImporterMetrics {
    fn default() -> Self {
        let block_height_gauge = Gauge::default();
        let latest_block_import_ms = Gauge::default();
        let execute_and_commit_duration = Histogram::new(buckets(Buckets::Timing));
        let gas_per_block = Gauge::default();
        let fee_per_block = Gauge::default();
        let transactions_per_block = Gauge::default();
        let gas_price = Gauge::default();

        let mut registry = global_registry().registry.lock();
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

        registry.register(
            "importer_gas_per_block",
            "The total gas used in a block",
            gas_per_block.clone(),
        );

        registry.register(
            "importer_fee_per_block_gwei",
            "The total fee (gwei) paid by transactions in a block",
            fee_per_block.clone(),
        );

        registry.register(
            "importer_transactions_per_block",
            "The total number of transactions in a block",
            transactions_per_block.clone(),
        );

        registry.register(
            "importer_gas_price_for_block",
            "The gas prices used in a block",
            gas_price.clone(),
        );

        Self {
            block_height: block_height_gauge,
            latest_block_import_timestamp: latest_block_import_ms,
            execute_and_commit_duration,
            gas_per_block,
            fee_per_block,
            transactions_per_block,
            gas_price,
        }
    }
}

// Setup a global static for accessing importer metrics
static IMPORTER_METRICS: OnceLock<ImporterMetrics> = OnceLock::new();

pub fn importer_metrics() -> &'static ImporterMetrics {
    IMPORTER_METRICS.get_or_init(ImporterMetrics::default)
}
