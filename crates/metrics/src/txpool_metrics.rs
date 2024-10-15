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
use std::sync::OnceLock;

pub struct TxPoolMetrics {
    /// Size of transactions in the txpool in bytes
    pub tx_size: Histogram,
    pub number_of_transactions: Gauge,
    pub number_of_transactions_pending_verification: Gauge,
    pub number_of_executable_transactions: Gauge,
    /// Time of transactions in the txpool in seconds
    pub transaction_time_in_txpool_secs: Histogram,
    /// Time actively spent by transaction insertion in the thread pool
    pub transaction_insertion_time_in_thread_pool_milliseconds: Histogram,
    /// How long it took for the selection algorithm to select transactions
    pub select_transaction_time_nanoseconds: Histogram,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let tx_size = Histogram::new(buckets(Buckets::TransactionSize));
        let transaction_time_in_txpool_secs = Histogram::new(buckets(Buckets::Timing));
        let select_transaction_time_nanoseconds =
            Histogram::new(buckets(Buckets::TimingCoarseGrained));
        let transaction_insertion_time_in_thread_pool_milliseconds =
            Histogram::new(buckets(Buckets::TimingCoarseGrained));

        let number_of_transactions = Gauge::default();
        let number_of_transactions_pending_verification = Gauge::default();
        let number_of_executable_transactions = Gauge::default();

        let metrics = TxPoolMetrics {
            tx_size,
            number_of_transactions,
            number_of_transactions_pending_verification,
            number_of_executable_transactions,
            transaction_time_in_txpool_secs,
            transaction_insertion_time_in_thread_pool_milliseconds,
            select_transaction_time_nanoseconds,
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
            "txpool_select_transaction_time_nanoseconds",
            "The time it took to select transactions for inclusion in a block in nanoseconds",
            metrics.select_transaction_time_nanoseconds.clone(),
        );

        registry.register(
            "txpool_insert_transaction_time_milliseconds",
            "The time it took to insert a transaction in the txpool in milliseconds",
            metrics.select_transaction_time_nanoseconds.clone(),
        );

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
