use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use fuel_core_types::fuel_tx::ContractId;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::Histogram,
    },
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
    // Block-level coherency + merge stage (coin/nonce verification and the
    // canonical fold), normalized by the block's tx count and gas.
    pub merge_ms: Histogram,
    pub merge_us_per_tx: Histogram,
    pub merge_ns_per_kgas: Histogram,
    // Per-contract `Changes` handoff, split into the take-into-worker side and
    // the re-insert-on-completion side, plus the shape of what was handed off.
    pub contract_handoff_split_us: Histogram,
    pub contract_handoff_merge_us: Histogram,
    pub contracts_per_batch: Histogram,
    pub handoff_changeset_keys: Histogram,
    // Sequential-fallback re-execution duration.
    pub sequential_fallback_ms: Histogram,
    // ---- time-spend visibility (per block) ------------------------------
    // Worker occupancy: total worker-seconds actually spent executing batch
    // inner work vs the worker-seconds available over the scheduler window
    // (worker_count x window). `busy / available` is the parallel utilization.
    pub worker_busy_seconds: Gauge<f64, AtomicU64>,
    pub worker_available_seconds: Gauge<f64, AtomicU64>,
    // Latency from scheduler start to the first batch dispatched onto a worker.
    pub time_to_first_dispatch_seconds: Gauge<f64, AtomicU64>,
    // Time the scheduler spent blocked asking the txpool for transactions
    // (`get_executable_transactions`), summed over the block.
    pub pool_ask_seconds: Gauge<f64, AtomicU64>,
    // Ask-protocol shape counters (cumulative): how many times the scheduler
    // asked the txpool for transactions, how many batches those asks returned,
    // and how many transactions they yielded in total. `asks / blocks` and
    // `txs / asks` are the yield-efficiency signals of the executor<->pool
    // protocol.
    pub pool_asks: Counter,
    pub pool_ask_batches: Counter,
    pub pool_ask_txs: Counter,
    // Per-ask lifecycle checkpoints (cumulative microseconds; divide by
    // `pool_asks` for means): queue = executor send -> pool worker starts;
    // in_pool = pool-side processing (scheduler + extraction; the finer split
    // lives in the txpool_lane_ask_* counters); return = pool response sent ->
    // executor resumes. The executor-side "ready for workers" stage is the
    // existing prepare phase metrics.
    pub pool_ask_queue_us: Counter,
    pub pool_ask_in_pool_us: Counter,
    pub pool_ask_return_us: Counter,
    // Decomposition of the scheduler's wall-clock production window into serial
    // phases (they sum to ~window); `execute_seconds` is the parallel
    // worker-busy time shown alongside for context, NOT part of the serial sum.
    pub phase_prepare_seconds: Gauge<f64, AtomicU64>,
    pub phase_execute_seconds: Gauge<f64, AtomicU64>,
    pub phase_handoff_seconds: Gauge<f64, AtomicU64>,
    pub phase_merge_seconds: Gauge<f64, AtomicU64>,
    pub phase_fallback_seconds: Gauge<f64, AtomicU64>,
    pub phase_idle_seconds: Gauge<f64, AtomicU64>,
    debug_batch_metrics_block_height: AtomicU64,
}

/// Per-block decomposition of where the scheduler spent its production window,
/// plus worker occupancy. Durations are gathered cheaply at the existing timing
/// sites and emitted once per block.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockTimeDecomposition {
    /// Total scheduler wall-clock window (`scheduler_run_time`).
    pub window: Duration,
    /// Batch preparation (`prepare_transactions_batch`), excluding the pool ask.
    pub prepare: Duration,
    /// Time blocked asking the txpool for transactions.
    pub pool_ask: Duration,
    /// Per-contract `Changes` handoff (split + merge sides).
    pub handoff: Duration,
    /// Block-level coherency + canonical fold.
    pub merge: Duration,
    /// Sequential-fallback re-execution.
    pub fallback: Duration,
    /// Latency from block start to the first batch dispatched.
    pub first_dispatch: Duration,
    /// Worker-seconds actually spent executing batch inner work.
    pub worker_busy: Duration,
    /// Worker-seconds available over the window (`worker_count x window`).
    pub worker_available: Duration,
}

impl BlockTimeDecomposition {
    /// Residual of the window not accounted for by the serial scheduler phases —
    /// time spent awaiting workers or the deadline with nothing serial to do.
    pub fn idle(&self) -> Duration {
        let serial = self
            .prepare
            .saturating_add(self.pool_ask)
            .saturating_add(self.handoff)
            .saturating_add(self.merge)
            .saturating_add(self.fallback);
        self.window.saturating_sub(serial)
    }
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
        let merge_ms = Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMs));
        let merge_us_per_tx =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMicrosecondsPerTx));
        let merge_ns_per_kgas = Histogram::new(buckets(
            Buckets::ParallelExecutorBatchTimeNanosecondsPerKGas,
        ));
        let contract_handoff_split_us =
            Histogram::new(buckets(Buckets::ParallelExecutorHandoffTimeMicroseconds));
        let contract_handoff_merge_us =
            Histogram::new(buckets(Buckets::ParallelExecutorHandoffTimeMicroseconds));
        let contracts_per_batch =
            Histogram::new(buckets(Buckets::ParallelExecutorContractsPerBatch));
        let handoff_changeset_keys =
            Histogram::new(buckets(Buckets::ParallelExecutorHandoffChangesetKeys));
        let sequential_fallback_ms =
            Histogram::new(buckets(Buckets::ParallelExecutorBatchTimeMs));

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
            merge_ms,
            merge_us_per_tx,
            merge_ns_per_kgas,
            contract_handoff_split_us,
            contract_handoff_merge_us,
            contracts_per_batch,
            handoff_changeset_keys,
            sequential_fallback_ms,
            worker_busy_seconds: Gauge::default(),
            worker_available_seconds: Gauge::default(),
            time_to_first_dispatch_seconds: Gauge::default(),
            pool_ask_seconds: Gauge::default(),
            pool_asks: Counter::default(),
            pool_ask_queue_us: Counter::default(),
            pool_ask_in_pool_us: Counter::default(),
            pool_ask_return_us: Counter::default(),
            pool_ask_batches: Counter::default(),
            pool_ask_txs: Counter::default(),
            phase_prepare_seconds: Gauge::default(),
            phase_execute_seconds: Gauge::default(),
            phase_handoff_seconds: Gauge::default(),
            phase_merge_seconds: Gauge::default(),
            phase_fallback_seconds: Gauge::default(),
            phase_idle_seconds: Gauge::default(),
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
        registry.register(
            "parallel_executor_merge_ms",
            "Time spent in the block-level coherency + merge stage (coin/nonce verification and the canonical fold) in milliseconds",
            metrics.merge_ms.clone(),
        );
        registry.register(
            "parallel_executor_merge_us_per_tx",
            "Block-level merge time in microseconds normalized by transactions",
            metrics.merge_us_per_tx.clone(),
        );
        registry.register(
            "parallel_executor_merge_ns_per_kgas",
            "Block-level merge time in nanoseconds normalized by 1000 gas",
            metrics.merge_ns_per_kgas.clone(),
        );
        registry.register(
            "parallel_executor_contract_handoff_split_us",
            "Time spent taking a batch's per-contract changes out of the shared map into the worker (split side) in microseconds",
            metrics.contract_handoff_split_us.clone(),
        );
        registry.register(
            "parallel_executor_contract_handoff_merge_us",
            "Time spent re-inserting a completed batch's per-contract changes into the shared map (merge side) in microseconds",
            metrics.contract_handoff_merge_us.clone(),
        );
        registry.register(
            "parallel_executor_contracts_per_batch",
            "Number of distinct contracts handed off in a batch",
            metrics.contracts_per_batch.clone(),
        );
        registry.register(
            "parallel_executor_handoff_changeset_keys",
            "Number of storage keys in a batch's handed-off per-contract change set",
            metrics.handoff_changeset_keys.clone(),
        );
        registry.register(
            "parallel_executor_sequential_fallback_ms",
            "Duration of a sequential-fallback re-execution in milliseconds",
            metrics.sequential_fallback_ms.clone(),
        );
        registry.register(
            "parallel_executor_worker_busy_seconds",
            "Total worker-seconds spent executing batch inner work in the last block",
            metrics.worker_busy_seconds.clone(),
        );
        registry.register(
            "parallel_executor_worker_available_seconds",
            "Worker-seconds available over the last block's scheduler window (worker_count x window)",
            metrics.worker_available_seconds.clone(),
        );
        registry.register(
            "parallel_executor_time_to_first_dispatch_seconds",
            "Latency from scheduler start to the first batch dispatched onto a worker",
            metrics.time_to_first_dispatch_seconds.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_seconds",
            "Time the scheduler spent blocked asking the txpool for transactions, summed over the block",
            metrics.pool_ask_seconds.clone(),
        );
        registry.register(
            "parallel_executor_pool_asks",
            "Number of transaction asks the scheduler sent to the txpool (cumulative)",
            metrics.pool_asks.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_batches",
            "Number of batches returned across all txpool asks (cumulative)",
            metrics.pool_ask_batches.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_txs",
            "Number of transactions returned across all txpool asks (cumulative)",
            metrics.pool_ask_txs.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_queue_us",
            "Cumulative time from executor ask sent to pool worker starting it (microseconds)",
            metrics.pool_ask_queue_us.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_in_pool_us",
            "Cumulative pool-side ask processing time (microseconds)",
            metrics.pool_ask_in_pool_us.clone(),
        );
        registry.register(
            "parallel_executor_pool_ask_return_us",
            "Cumulative time from pool response sent to executor resuming (microseconds)",
            metrics.pool_ask_return_us.clone(),
        );
        registry.register(
            "parallel_executor_phase_prepare_seconds",
            "Block-window phase: batch preparation (excluding the txpool ask)",
            metrics.phase_prepare_seconds.clone(),
        );
        registry.register(
            "parallel_executor_phase_execute_seconds",
            "Block-window phase: parallel worker-busy seconds (shown for context; not part of the serial sum)",
            metrics.phase_execute_seconds.clone(),
        );
        registry.register(
            "parallel_executor_phase_handoff_seconds",
            "Block-window phase: per-contract Changes handoff (split + merge sides)",
            metrics.phase_handoff_seconds.clone(),
        );
        registry.register(
            "parallel_executor_phase_merge_seconds",
            "Block-window phase: block-level coherency + canonical fold",
            metrics.phase_merge_seconds.clone(),
        );
        registry.register(
            "parallel_executor_phase_fallback_seconds",
            "Block-window phase: sequential-fallback re-execution",
            metrics.phase_fallback_seconds.clone(),
        );
        registry.register(
            "parallel_executor_phase_idle_seconds",
            "Block-window phase: residual window spent awaiting workers or the deadline",
            metrics.phase_idle_seconds.clone(),
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

pub fn set_non_empty_batch_transactions(block_height: u64, batch_tx_counts: &[u32]) {
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

/// Record one executor->txpool transaction ask: how many batches it returned
/// and how many transactions those batches carried in total.
pub fn record_pool_ask(batches: usize, txs: usize) {
    let metrics = parallel_executor_metrics();
    metrics.pool_asks.inc();
    metrics.pool_ask_batches.inc_by(batches as u64);
    metrics.pool_ask_txs.inc_by(txs as u64);
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

/// Record the block-level coherency + merge stage (coin/nonce verification and
/// the canonical fold), normalized by the block's tx count and gas.
pub fn record_block_merge(duration: Duration, tx_count: u32, gas: u64) {
    let metrics = parallel_executor_metrics();
    record_batch_time(
        duration,
        tx_count,
        gas,
        &metrics.merge_ms,
        &metrics.merge_us_per_tx,
        &metrics.merge_ns_per_kgas,
    );
}

/// Record the split side of a batch's per-contract `Changes` handoff (taking the
/// accumulated changes out of the shared map into the worker), together with the
/// shape of what was handed off (contract count and total storage keys).
pub fn record_contract_handoff_split(
    duration: Duration,
    contract_count: usize,
    changeset_keys: usize,
) {
    let metrics = parallel_executor_metrics();
    metrics
        .contract_handoff_split_us
        .observe(duration_us(duration));
    if contract_count > 0 {
        metrics.contracts_per_batch.observe(contract_count as f64);
        metrics
            .handoff_changeset_keys
            .observe(changeset_keys as f64);
    }
}

/// Record the merge side of a batch's per-contract `Changes` handoff
/// (re-inserting a completed batch's changes into the shared map).
pub fn record_contract_handoff_merge(duration: Duration) {
    parallel_executor_metrics()
        .contract_handoff_merge_us
        .observe(duration_us(duration));
}

/// Record the duration of a sequential-fallback re-execution.
pub fn record_sequential_fallback(duration: Duration) {
    parallel_executor_metrics()
        .sequential_fallback_ms
        .observe(duration_ms(duration));
}

/// Record the per-block time-spend decomposition (worker occupancy,
/// time-to-first-dispatch, txpool ask time, and the production-window phase
/// breakdown). Called once at the end of each block's scheduler run.
pub fn record_block_time_decomposition(d: BlockTimeDecomposition) {
    let metrics = parallel_executor_metrics();
    metrics.worker_busy_seconds.set(d.worker_busy.as_secs_f64());
    metrics
        .worker_available_seconds
        .set(d.worker_available.as_secs_f64());
    metrics
        .time_to_first_dispatch_seconds
        .set(d.first_dispatch.as_secs_f64());
    metrics.pool_ask_seconds.set(d.pool_ask.as_secs_f64());
    metrics.phase_prepare_seconds.set(d.prepare.as_secs_f64());
    metrics
        .phase_execute_seconds
        .set(d.worker_busy.as_secs_f64());
    metrics.phase_handoff_seconds.set(d.handoff.as_secs_f64());
    metrics.phase_merge_seconds.set(d.merge.as_secs_f64());
    metrics.phase_fallback_seconds.set(d.fallback.as_secs_f64());
    metrics.phase_idle_seconds.set(d.idle().as_secs_f64());
}
