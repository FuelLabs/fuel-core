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
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let tx_sizes = Vec::new(); // TODO: What values for tx_sizes?
        let tx_size = Histogram::new(tx_sizes.into_iter());

        let number_of_transactions = Gauge::default();
        let number_of_transactions_pending_verification = Gauge::default();
        let metrics = TxPoolMetrics {
            tx_size,
            number_of_transactions,
            number_of_transactions_pending_verification,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "Tx_Size_Histogram",
            "A Histogram keeping track of the size of transactions in bytes",
            metrics.tx_size.clone(),
        );

        registry.register(
            "Number_Of_Transactions_Gaguge",
            "A Gauge keeping track of the number of transactions in the mempool",
            metrics.number_of_transactions.clone(),
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
