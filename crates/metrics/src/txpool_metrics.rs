use crate::{
    buckets::{
        Buckets,
        buckets,
    },
    global_registry,
};
use prometheus_client::metrics::{
    counter::Counter,
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::OnceLock;

pub struct TxPoolMetrics {
    /// Number of lane-scheduler extraction requests answered.
    pub lane_ask_count: Counter,
    /// Cumulative in-pool time answering lane asks, microseconds.
    pub lane_ask_total_us: Counter,
    /// Cumulative lane-scheduler `next_batches` compute, microseconds.
    pub lane_ask_scheduler_us: Counter,
    /// Cumulative storage graph-removal time within lane asks, microseconds.
    pub lane_ask_removal_us: Counter,
    /// Cumulative cache-bookkeeping time within lane asks, microseconds.
    pub lane_ask_caches_us: Counter,
    /// Cumulative transactions extracted by lane asks.
    pub lane_ask_extracted: Counter,
    /// Size of transactions in the txpool in bytes
    pub tx_size: Histogram,
    /// Number of transactions in the txpool
    pub number_of_transactions: Gauge,
    /// Number of transactions pending verification, before being inserted in the txpool
    pub number_of_transactions_pending_verification: Gauge,
    /// Number of transactions that can be included in the next block
    pub number_of_executable_transactions: Gauge,
    /// Time of transactions in the txpool in seconds
    pub transaction_time_in_txpool_secs: Histogram,
    /// Time actively spent by transaction insertion in the thread pool
    pub transaction_insertion_time_in_thread_pool_microseconds: Histogram,
    /// How long it took for the selection algorithm to select transactions
    pub select_transactions_time_microseconds: Histogram,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let tx_size = Histogram::new(buckets(Buckets::TransactionSize));
        let transaction_time_in_txpool_secs =
            Histogram::new(buckets(Buckets::TransactionTimeInTxpool));
        let select_transactions_time_microseconds =
            Histogram::new(buckets(Buckets::SelectTransactionsTime));
        let transaction_insertion_time_in_thread_pool_microseconds =
            Histogram::new(buckets(Buckets::TransactionInsertionTimeInThreadPool));

        let number_of_transactions = Gauge::default();
        let number_of_transactions_pending_verification = Gauge::default();
        let number_of_executable_transactions = Gauge::default();

        let metrics = TxPoolMetrics {
            lane_ask_count: Counter::default(),
            lane_ask_total_us: Counter::default(),
            lane_ask_scheduler_us: Counter::default(),
            lane_ask_removal_us: Counter::default(),
            lane_ask_caches_us: Counter::default(),
            lane_ask_extracted: Counter::default(),
            tx_size,
            number_of_transactions,
            number_of_transactions_pending_verification,
            number_of_executable_transactions,
            transaction_time_in_txpool_secs,
            transaction_insertion_time_in_thread_pool_microseconds,
            select_transactions_time_microseconds,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "txpool_tx_size",
            "The size of transactions in the txpool",
            metrics.tx_size.clone(),
        );

        registry.register(
            "txpool_tx_time_in_txpool_seconds",
            "The time spent by a transaction in the txpool in seconds",
            metrics.transaction_time_in_txpool_secs.clone(),
        );

        registry.register(
            "txpool_number_of_transactions",
            "The number of transactions in the txpool",
            metrics.number_of_transactions.clone(),
        );

        registry.register(
            "txpool_number_of_executable_transactions",
            "The number of executable transactions in the txpool",
            metrics.number_of_executable_transactions.clone(),
        );

        registry.register(
            "txpool_number_of_transactions_pending_verification",
            "The number of transactions pending verification before entering the txpool",
            metrics.number_of_transactions_pending_verification.clone(),
        );

        registry.register(
            "txpool_select_transactions_time_microseconds",
            "The time it took to select transactions for inclusion in a block in microseconds",
            metrics.select_transactions_time_microseconds.clone(),
        );

        registry.register(
            "txpool_lane_ask_count",
            "Number of lane-scheduler extraction requests answered",
            metrics.lane_ask_count.clone(),
        );
        registry.register(
            "txpool_lane_ask_total_us",
            "Cumulative in-pool time answering lane asks (microseconds)",
            metrics.lane_ask_total_us.clone(),
        );
        registry.register(
            "txpool_lane_ask_scheduler_us",
            "Cumulative lane-scheduler next_batches compute (microseconds)",
            metrics.lane_ask_scheduler_us.clone(),
        );
        registry.register(
            "txpool_lane_ask_removal_us",
            "Cumulative storage graph-removal time within lane asks (microseconds)",
            metrics.lane_ask_removal_us.clone(),
        );
        registry.register(
            "txpool_lane_ask_caches_us",
            "Cumulative cache-bookkeeping time within lane asks (microseconds)",
            metrics.lane_ask_caches_us.clone(),
        );
        registry.register(
            "txpool_lane_ask_extracted",
            "Cumulative transactions extracted by lane asks",
            metrics.lane_ask_extracted.clone(),
        );

        registry.register(
            "txpool_transaction_insertion_time_in_thread_pool_microseconds",
            "The time it took to insert a transaction in the txpool in microseconds",
            metrics
                .transaction_insertion_time_in_thread_pool_microseconds
                .clone(),
        );

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
