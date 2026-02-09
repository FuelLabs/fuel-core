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
use std::{
    sync::{
        OnceLock,
        atomic::AtomicU64,
    },
    time::Duration,
};

// TODO: We don't need all of these maybe. And some should be histograms, but I'm just using it for
// benchmarks
pub struct ParallelExecutorMetrics {
    pub execution_time_seconds: Gauge<f64, AtomicU64>,
    pub number_of_transactions: Gauge,
    pub total_gas_used: Gauge,
    pub block_height: Gauge,
    pub max_workers_used: Gauge,
    pub batch_prepare_ms: Histogram,
    pub batch_prepare_us_per_tx: Histogram,
    pub batch_prepare_ns_per_kgas: Histogram,
    pub batch_execute_ms: Histogram,
    pub batch_execute_us_per_tx: Histogram,
    pub batch_execute_ns_per_kgas: Histogram,
    pub batch_total_ms: Histogram,
    pub batch_total_us_per_tx: Histogram,
    pub batch_total_ns_per_kgas: Histogram,
}

impl Default for ParallelExecutorMetrics {
    fn default() -> Self {
        let execution_time_seconds = Gauge::default();
        let number_of_transactions = Gauge::default();
        let total_gas_used = Gauge::default();
        let block_height = Gauge::default();
        let max_workers_used = Gauge::default();
        let batch_prepare_ms =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMs));
        let batch_prepare_us_per_tx =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMicrosecondsPerTx));
        let batch_prepare_ns_per_kgas = Histogram::new(buckets(
            Buckets::ParallelExecutorBatchTimeNanosecondsPerKGas,
        ));
        let batch_execute_ms =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMs));
        let batch_execute_us_per_tx =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMicrosecondsPerTx));
        let batch_execute_ns_per_kgas = Histogram::new(buckets(
            Buckets::ParallelExecutorBatchTimeNanosecondsPerKGas,
        ));
        let batch_total_ms =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMs));
        let batch_total_us_per_tx =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMicrosecondsPerTx));
        let batch_total_ns_per_kgas = Histogram::new(buckets(
            Buckets::ParallelExecutorBatchTimeNanosecondsPerKGas,
        ));

        let metrics = ParallelExecutorMetrics {
            execution_time_seconds,
            number_of_transactions,
            total_gas_used,
            block_height,
            max_workers_used,
            batch_prepare_ms,
            batch_prepare_us_per_tx,
            batch_prepare_ns_per_kgas,
            batch_execute_ms,
            batch_execute_us_per_tx,
            batch_execute_ns_per_kgas,
            batch_total_ms,
            batch_total_us_per_tx,
            batch_total_ns_per_kgas,
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
            "parallel_executor_total_gas_used",
            "Total gas used by transactions executed by the parallel executor",
            metrics.total_gas_used.clone(),
        );
        registry.register(
            "parallel_executor_block_height",
            "Block height for the parallel executor metrics sample",
            metrics.block_height.clone(),
        );
        registry.register(
            "parallel_executor_max_workers_used",
            "Maximum number of workers used concurrently by the parallel executor per block",
            metrics.max_workers_used.clone(),
        );
        registry.register(
            "parallel_executor_batch_prepare_ms",
            "Time spent preparing a batch in milliseconds",
            metrics.batch_prepare_ms.clone(),
        );
        registry.register(
            "parallel_executor_batch_prepare_us_per_tx",
            "Time spent preparing a batch in microseconds normalized by transactions",
            metrics.batch_prepare_us_per_tx.clone(),
        );
        registry.register(
            "parallel_executor_batch_prepare_ns_per_kgas",
            "Time spent preparing a batch in nanoseconds normalized by 1000 gas",
            metrics.batch_prepare_ns_per_kgas.clone(),
        );
        registry.register(
            "parallel_executor_batch_execute_ms",
            "Time spent executing a batch in milliseconds",
            metrics.batch_execute_ms.clone(),
        );
        registry.register(
            "parallel_executor_batch_execute_us_per_tx",
            "Time spent executing a batch in microseconds normalized by transactions",
            metrics.batch_execute_us_per_tx.clone(),
        );
        registry.register(
            "parallel_executor_batch_execute_ns_per_kgas",
            "Time spent executing a batch in nanoseconds normalized by 1000 gas",
            metrics.batch_execute_ns_per_kgas.clone(),
        );
        registry.register(
            "parallel_executor_batch_total_ms",
            "Total time spent preparing and executing a batch in milliseconds",
            metrics.batch_total_ms.clone(),
        );
        registry.register(
            "parallel_executor_batch_total_us_per_tx",
            "Total time spent preparing and executing a batch in microseconds normalized by transactions",
            metrics.batch_total_us_per_tx.clone(),
        );
        registry.register(
            "parallel_executor_batch_total_ns_per_kgas",
            "Total time spent preparing and executing a batch in nanoseconds normalized by 1000 gas",
            metrics.batch_total_ns_per_kgas.clone(),
        );

        metrics
    }
}

static PARALLEL_EXECUTOR_METRICS: OnceLock<ParallelExecutorMetrics> = OnceLock::new();

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

pub fn set_total_gas_used(gas: u64) {
    parallel_executor_metrics().total_gas_used.set(gas as i64);
}

pub fn set_block_height(height: u32) {
    parallel_executor_metrics().block_height.set(height as i64);
}

pub fn set_max_workers_used(max_workers_used: u32) {
    parallel_executor_metrics()
        .max_workers_used
        .set(max_workers_used as i64);
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn duration_us(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000.0
}

fn duration_ns(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000_000.0
}

fn record_batch_time(
    duration: Duration,
    tx_count: u32,
    gas: u64,
    raw: &Histogram,
    per_tx: &Histogram,
    per_gas: &Histogram,
) {
    let duration_ms = duration_ms(duration);
    raw.observe(duration_ms);
    let duration_us = duration_us(duration);
    if tx_count > 0 {
        per_tx.observe(duration_us / f64::from(tx_count));
    }
    let gas_in_kgas = gas as f64 / 1000.0;
    if gas_in_kgas > 0.0 {
        per_gas.observe(duration_ns(duration) / gas_in_kgas);
    }
}

pub fn record_batch_prepare(duration: Duration, tx_count: u32, gas: u64) {
    let metrics = parallel_executor_metrics();
    record_batch_time(
        duration,
        tx_count,
        gas,
        &metrics.batch_prepare_ms,
        &metrics.batch_prepare_us_per_tx,
        &metrics.batch_prepare_ns_per_kgas,
    );
}

pub fn record_batch_execute(duration: Duration, tx_count: u32, gas: u64) {
    let metrics = parallel_executor_metrics();
    record_batch_time(
        duration,
        tx_count,
        gas,
        &metrics.batch_execute_ms,
        &metrics.batch_execute_us_per_tx,
        &metrics.batch_execute_ns_per_kgas,
    );
}

pub fn record_batch_total(duration: Duration, tx_count: u32, gas: u64) {
    let metrics = parallel_executor_metrics();
    record_batch_time(
        duration,
        tx_count,
        gas,
        &metrics.batch_total_ms,
        &metrics.batch_total_us_per_tx,
        &metrics.batch_total_ns_per_kgas,
    );
}
