use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use fuel_core_types::fuel_tx::ContractId;
use prometheus_client::metrics::{
    family::Family,
    gauge::Gauge,
    histogram::Histogram,
};
use prometheus_client::encoding::EncodeLabelSet;
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
    pub hot_contracts_tracked: Gauge,
    pub complex_txs_classified: Gauge,
    pub complex_txs_selected: Gauge,
    pub complex_txs_remaining: Gauge,
    pub non_empty_batches: Gauge,
    pub non_empty_batch_transactions: Family<BatchMetricLabel, Gauge>,
    pub non_empty_batch_allocated_gas: Family<BatchMetricLabel, Gauge>,
    pub non_empty_batch_used_gas: Family<BatchMetricLabel, Gauge>,
    pub batch_anchor_contracts: Family<BatchAnchorLabel, Gauge>,
    pub block_production_time_seconds: Gauge<f64, AtomicU64>,
    pub scheduler_run_time_seconds: Gauge<f64, AtomicU64>,
    pub batch_prepare_ms: Histogram,
    pub batch_prepare_us_per_tx: Histogram,
    pub batch_prepare_ns_per_kgas: Histogram,
    pub batch_execute_ms: Histogram,
    pub batch_execute_us_per_tx: Histogram,
    pub batch_execute_ns_per_kgas: Histogram,
    pub batch_total_ms: Histogram,
    pub batch_total_us_per_tx: Histogram,
    pub batch_total_ns_per_kgas: Histogram,
    debug_batch_metrics_block_height: AtomicU64,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct BatchMetricLabel {
    pub block_height: u64,
    pub batch_index: u64,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct BatchAnchorLabel {
    pub block_height: u64,
    pub batch_index: u64,
    pub contract_id: String,
}

impl Default for ParallelExecutorMetrics {
    fn default() -> Self {
        let execution_time_seconds = Gauge::default();
        let number_of_transactions = Gauge::default();
        let total_gas_used = Gauge::default();
        let block_height = Gauge::default();
        let max_workers_used = Gauge::default();
        let hot_contracts_tracked = Gauge::default();
        let complex_txs_classified = Gauge::default();
        let complex_txs_selected = Gauge::default();
        let complex_txs_remaining = Gauge::default();
        let non_empty_batches = Gauge::default();
        let non_empty_batch_transactions = Family::default();
        let non_empty_batch_allocated_gas = Family::default();
        let non_empty_batch_used_gas = Family::default();
        let batch_anchor_contracts = Family::default();
        let block_production_time_seconds = Gauge::default();
        let scheduler_run_time_seconds = Gauge::default();
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
            hot_contracts_tracked,
            complex_txs_classified,
            complex_txs_selected,
            complex_txs_remaining,
            non_empty_batches,
            non_empty_batch_transactions,
            non_empty_batch_allocated_gas,
            non_empty_batch_used_gas,
            batch_anchor_contracts,
            block_production_time_seconds,
            scheduler_run_time_seconds,
            batch_prepare_ms,
            batch_prepare_us_per_tx,
            batch_prepare_ns_per_kgas,
            batch_execute_ms,
            batch_execute_us_per_tx,
            batch_execute_ns_per_kgas,
            batch_total_ms,
            batch_total_us_per_tx,
            batch_total_ns_per_kgas,
            debug_batch_metrics_block_height: AtomicU64::new(0),
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
            "parallel_executor_hot_contracts_tracked",
            "Number of distinct hot anchor contracts currently tracked in the hot contract cache",
            metrics.hot_contracts_tracked.clone(),
        );
        registry.register(
            "parallel_executor_complex_txs_classified",
            "Number of transactions classified as complex during the latest tx selection pass",
            metrics.complex_txs_classified.clone(),
        );
        registry.register(
            "parallel_executor_complex_txs_selected",
            "Number of deferred complex transactions selected into the latest complex-only pass",
            metrics.complex_txs_selected.clone(),
        );
        registry.register(
            "parallel_executor_complex_txs_remaining",
            "Number of deferred complex transactions remaining after the latest tx selection pass",
            metrics.complex_txs_remaining.clone(),
        );
        registry.register(
            "parallel_executor_non_empty_batches",
            "Number of non-empty transaction batches created by the parallel executor per block",
            metrics.non_empty_batches.clone(),
        );
        registry.register(
            "parallel_executor_non_empty_batch_transactions",
            "Exact transaction counts for each non-empty batch keyed by synthetic block_height and batch_index",
            metrics.non_empty_batch_transactions.clone(),
        );
        registry.register(
            "parallel_executor_non_empty_batch_allocated_gas",
            "Allocated gas for each non-empty batch keyed by synthetic block_height and batch_index",
            metrics.non_empty_batch_allocated_gas.clone(),
        );
        registry.register(
            "parallel_executor_non_empty_batch_used_gas",
            "Used gas for each non-empty batch keyed by synthetic block_height and batch_index",
            metrics.non_empty_batch_used_gas.clone(),
        );
        registry.register(
            "parallel_executor_batch_anchor_contract",
            "Anchor contract ids chosen for each non-empty batch keyed by synthetic block_height and batch_index",
            metrics.batch_anchor_contracts.clone(),
        );
        registry.register(
            "parallel_executor_block_production_time_seconds",
            "Time spent producing blocks after transactions are added to the block",
            metrics.block_production_time_seconds.clone(),
        );
        registry.register(
            "parallel_executor_scheduler_run_time_seconds",
            "Total time spent running the parallel executor scheduler",
            metrics.scheduler_run_time_seconds.clone(),
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

pub fn set_hot_contracts_tracked(count: usize) {
    parallel_executor_metrics()
        .hot_contracts_tracked
        .set(i64::try_from(count).unwrap_or(i64::MAX));
}

pub fn set_complex_txs_classified(count: usize) {
    parallel_executor_metrics()
        .complex_txs_classified
        .set(i64::try_from(count).unwrap_or(i64::MAX));
}

pub fn set_complex_txs_selected(count: usize) {
    parallel_executor_metrics()
        .complex_txs_selected
        .set(i64::try_from(count).unwrap_or(i64::MAX));
}

pub fn set_complex_txs_remaining(count: usize) {
    parallel_executor_metrics()
        .complex_txs_remaining
        .set(i64::try_from(count).unwrap_or(i64::MAX));
}

pub fn next_debug_batch_metrics_block_height() -> u64 {
    // TODO: Replace this synthetic block id with a real block/run identifier before merge.
    parallel_executor_metrics()
        .debug_batch_metrics_block_height
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .saturating_add(1)
}

pub fn set_non_empty_batch_transactions(
    block_height: u64,
    batch_tx_counts: &[u32],
) {
    let metrics = parallel_executor_metrics();
    metrics
        .non_empty_batches
        .set(i64::try_from(batch_tx_counts.len()).unwrap_or(i64::MAX));

    for (batch_index, tx_count) in batch_tx_counts.iter().enumerate() {
        metrics
            .non_empty_batch_transactions
            .get_or_create(&BatchMetricLabel {
                block_height,
                batch_index: u64::try_from(batch_index).unwrap_or(u64::MAX),
            })
            .set(i64::from(*tx_count));
    }
}

pub fn set_non_empty_batch_allocated_gas(block_height: u64, batch_gas: &[u64]) {
    let metrics = parallel_executor_metrics();
    for (batch_index, gas) in batch_gas.iter().enumerate() {
        metrics
            .non_empty_batch_allocated_gas
            .get_or_create(&BatchMetricLabel {
                block_height,
                batch_index: u64::try_from(batch_index).unwrap_or(u64::MAX),
            })
            .set(i64::try_from(*gas).unwrap_or(i64::MAX));
    }
}

pub fn set_non_empty_batch_used_gas(block_height: u64, batch_gas: &[u64]) {
    let metrics = parallel_executor_metrics();
    for (batch_index, gas) in batch_gas.iter().enumerate() {
        metrics
            .non_empty_batch_used_gas
            .get_or_create(&BatchMetricLabel {
                block_height,
                batch_index: u64::try_from(batch_index).unwrap_or(u64::MAX),
            })
            .set(i64::try_from(*gas).unwrap_or(i64::MAX));
    }
}

pub fn set_batch_anchor_contracts(
    block_height: u64,
    batch_anchor_contracts: &[Vec<ContractId>],
) {
    let metrics = parallel_executor_metrics();
    for (batch_index, anchors) in batch_anchor_contracts.iter().enumerate() {
        for contract_id in anchors {
            metrics
                .batch_anchor_contracts
                .get_or_create(&BatchAnchorLabel {
                    block_height,
                    batch_index: u64::try_from(batch_index).unwrap_or(u64::MAX),
                    contract_id: contract_id.to_string(),
                })
                .set(1);
        }
    }
}

pub fn record_block_production_time(duration: Duration) {
    parallel_executor_metrics()
        .block_production_time_seconds
        .set(duration.as_secs_f64());
}

pub fn record_scheduler_run_time(duration: Duration) {
    parallel_executor_metrics()
        .scheduler_run_time_seconds
        .set(duration.as_secs_f64());
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
