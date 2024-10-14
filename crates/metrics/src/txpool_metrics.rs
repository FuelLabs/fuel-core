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
            Histogram::new(buckets(Buckets::Timing));
        let transaction_insertion_time_in_thread_pool_milliseconds =
            Histogram::new(buckets(Buckets::Timing));

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
            "Tx_Size_Histogram",
            "A Histogram keeping track of the size of transactions in bytes",
            metrics.tx_size.clone(),
        );

        registry.register(
            "Tx_Time_in_Txpool_Histogram",
            "A Histogram keeping track of the time spent by transactions in the txpool in seconds",
            metrics.transaction_time_in_txpool_secs.clone(),
        );

        registry.register(
            "Number_Of_Transactions_Gauge",
            "A Gauge keeping track of the number of transactions in the txpool",
            metrics.number_of_transactions.clone(),
        );

        registry.register(
            "Number_Of_Executable_Transactions_Gauge",
            "A Gauge keeping track of the number of executable transactions",
            metrics.number_of_executable_transactions.clone(),
        );

        registry.register(
            "Number_Of_Transactions_Pending_Verification_Gaguge",
            "A Gauge keeping track of the number of transactions pending verification",
            metrics.number_of_transactions_pending_verification.clone(),
        );

        registry.register(
            "Select_Transaction_Time_Nanoseconds",
            "How long in nanoseconds it took for the selection algorithm to select transactions",
            metrics.select_transaction_time_nanoseconds.clone(),
        );

        registry.register(
            "Insert_Transaction_Time_Milliseconds",
            "Time spent by transaction insertion function in the rayon thread pool in milliseconds",
            metrics.select_transaction_time_nanoseconds.clone(),
        );

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
