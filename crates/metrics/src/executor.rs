use crate::{
    buckets::{
        buckets,
        Buckets,
    },
    global_registry,
};
use prometheus_client::metrics::histogram::Histogram;
use std::sync::OnceLock;

pub struct ExecutorMetrics {
    pub gas_per_block: Histogram,
    pub fee_per_block: Histogram,
    pub transactions_per_block: Histogram,
    pub size_per_block_bytes: Histogram,
}

impl Default for ExecutorMetrics {
    fn default() -> Self {
        let gas_per_block = Histogram::new(buckets(Buckets::GasUsed));
        let fee_per_block = Histogram::new(buckets(Buckets::Fee));
        let transactions_per_block = Histogram::new(buckets(Buckets::TransactionsCount));
        let size_per_block_bytes = Histogram::new(buckets(Buckets::SizeUsed));

        let mut registry = global_registry().registry.lock();
        registry.register(
            "executor_gas_per_block",
            "The total gas used in a block",
            gas_per_block.clone(),
        );

        registry.register(
            "executor_fee_per_block_gwei",
            "The total fee (gwei) paid by transactions in a block",
            fee_per_block.clone(),
        );

        registry.register(
            "executor_transactions_per_block",
            "The total number of transactions in a block",
            transactions_per_block.clone(),
        );

        registry.register(
            "executor_size_per_block_bytes",
            "The size of a block in bytes",
            transactions_per_block.clone(),
        );

        Self {
            gas_per_block,
            fee_per_block,
            transactions_per_block,
            size_per_block_bytes,
        }
    }
}

// Setup a global static for accessing importer metrics
static EXECUTOR_METRICS: OnceLock<ExecutorMetrics> = OnceLock::new();

pub fn executor_metrics() -> &'static ExecutorMetrics {
    EXECUTOR_METRICS.get_or_init(ExecutorMetrics::default)
}
