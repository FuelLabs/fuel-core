use lazy_static::lazy_static;
use prometheus_client::{
    metrics::gauge::Gauge,
    registry::Registry,
};

pub struct ImporterMetrics {
    pub registry: Registry,
    // using gauge in case txs have to be rolled back for any reason
    pub total_txs_count: Gauge,
}

impl Default for ImporterMetrics {
    fn default() -> Self {
        let mut registry = Registry::default();

        let tx_count_gauge = Gauge::default();

        registry.register(
            "importer_tx_count",
            "the total amount of transactions that have been imported on chain",
            tx_count_gauge.clone(),
        );

        Self {
            registry,
            total_txs_count: tx_count_gauge,
        }
    }
}

lazy_static! {
    pub static ref IMPORTER_METRICS: ImporterMetrics = ImporterMetrics::default();
}
