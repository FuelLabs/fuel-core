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
        let transaction_time_in_txpool_secs = Histogram::new(buckets(Buckets::Timing));
        let select_transactions_time_microseconds =
            Histogram::new(buckets(Buckets::TimingCoarseGrained));
        let transaction_insertion_time_in_thread_pool_microseconds =
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
