use crate::global_registry;
use prometheus_client::metrics::{
    gauge::Gauge,
    histogram::Histogram,
};
use std::sync::OnceLock;

pub struct TxPoolMetrics {
    pub tx_size_histogram: Histogram,
    pub number_of_transactions_gauge: Gauge,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let tx_sizes = Vec::new(); // TODO: What values for tx_sizes?
        let tx_size_histogram = Histogram::new(tx_sizes.into_iter());

        let number_of_transactions_gauge = Gauge::default();
        let metrics = TxPoolMetrics {
            tx_size_histogram,
            number_of_transactions_gauge,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "Tx_Size_Histogram",
            "A Histogram keeping track of the size of transactions in bytes",
            metrics.tx_size_histogram.clone(),
        );

        registry.register(
            "Number_Of_Transactions_Gaguge",
            "A Gauge keeping track of the number of transactions in the mempool",
            metrics.number_of_transactions_gauge.clone(),
        );

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
