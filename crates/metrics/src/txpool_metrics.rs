use crate::global_registry;
use prometheus_client::metrics::{
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::OnceLock;

pub struct TxPoolMetrics {
    pub tx_size: Histogram,
    pub number_of_transactions: Gauge,
    pub number_of_transactions_pending_verification: Gauge,
    pub number_of_executable_transactions: Gauge,
    pub transaction_time_in_txpool: Histogram,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let tx_sizes = Vec::new(); // TODO: What values for tx_sizes?
        let tx_size = Histogram::new(tx_sizes.into_iter());
        let transaction_time_in_txpool = Histogram::new(
            vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 15.0, 20.0, 25.0,
                30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0,
            ]
            .into_iter(),
        );

        let number_of_transactions = Gauge::default();
        let number_of_transactions_pending_verification = Gauge::default();
        let number_of_executable_transactions = Gauge::default();
        let metrics = TxPoolMetrics {
            tx_size,
            number_of_transactions,
            number_of_transactions_pending_verification,
            number_of_executable_transactions,
            transaction_time_in_txpool,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "Tx_Size_Histogram",
            "A Histogram keeping track of the size of transactions in bytes",
            metrics.tx_size.clone(),
        );

        registry.register(
            "Tx_Time_in_Txpool_Histogram",
            "A Histogram keeping track of the time spent by transactions in the txpools",
            metrics.transaction_time_in_txpool.clone(),
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

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
