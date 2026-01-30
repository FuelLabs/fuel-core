use crate::global_registry;
use prometheus_client::metrics::{
    gauge::Gauge,
};
use std::{
    sync::OnceLock,
    sync::atomic::AtomicU64,
    time::Duration,
};

pub struct ParallelExecutorMetrics {
    pub execution_time_seconds: Gauge<f64, AtomicU64>,
    pub number_of_transactions: Gauge,
    pub block_height: Gauge,
}

impl Default for ParallelExecutorMetrics {
    fn default() -> Self {
        let execution_time_seconds = Gauge::default();
        let number_of_transactions = Gauge::default();
        let block_height = Gauge::default();

        let metrics = ParallelExecutorMetrics {
            execution_time_seconds,
            number_of_transactions,
            block_height,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "parallel_executor_execution_time_seconds",
            "Time spent executing transactions in the parallel executor in seconds",
            metrics.execution_time_seconds.clone(),
        );
        registry.register(
            "parallel_executor_number_of_transactions",
            "Number of transactions executed by the parallel executor",
            metrics.number_of_transactions.clone(),
        );
        registry.register(
            "parallel_executor_block_height",
            "Block height for the parallel executor metrics sample",
            metrics.block_height.clone(),
        );

        metrics
    }
}

static PARALLEL_EXECUTOR_METRICS: OnceLock<ParallelExecutorMetrics> =
    OnceLock::new();

pub fn parallel_executor_metrics() -> &'static ParallelExecutorMetrics {
    PARALLEL_EXECUTOR_METRICS.get_or_init(ParallelExecutorMetrics::default)
}

pub fn record_execution_time(duration: Duration) {
    parallel_executor_metrics()
        .execution_time_seconds
        .set(duration.as_secs_f64());
}

pub fn set_number_of_transactions(count: u32) {
    parallel_executor_metrics()
        .number_of_transactions
        .set(count as i64);
}

pub fn set_block_height(height: u32) {
    parallel_executor_metrics()
        .block_height
        .set(height as i64);
}
