use prometheus_client::{
    metrics::histogram::Histogram,
    registry::Registry,
};
use std::sync::OnceLock;

pub struct TxPoolMetrics {
    // Attaches each Metric to the Registry
    pub registry: Registry,
    pub tx_size_histogram: Histogram,
}

impl Default for TxPoolMetrics {
    fn default() -> Self {
        let registry = Registry::default();

        let tx_sizes = Vec::new();

        let tx_size_histogram = Histogram::new(tx_sizes.into_iter());

        let mut metrics = TxPoolMetrics {
            registry,
            tx_size_histogram,
        };

        metrics.registry.register(
            "Tx_Size_Histogram",
            "A Histogram keeping track of the size of txs",
            metrics.tx_size_histogram.clone(),
        );

        metrics
    }
}

static TXPOOL_METRICS: OnceLock<TxPoolMetrics> = OnceLock::new();
pub fn txpool_metrics() -> &'static TxPoolMetrics {
    TXPOOL_METRICS.get_or_init(TxPoolMetrics::default)
}
