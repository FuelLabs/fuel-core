use crate::{
    buckets,
    global_registry,
    Buckets,
};
use prometheus_client::metrics::histogram::Histogram;
use std::sync::OnceLock;

pub struct ProducerMetrics {
    pub gas_price: Histogram,
}

impl Default for ProducerMetrics {
    fn default() -> Self {
        let gas_price = Histogram::new(buckets(Buckets::GasPrice));

        let mut registry = global_registry().registry.lock();

        registry.register(
            "gas_price_per_block",
            "The gas price of a block",
            gas_price.clone(),
        );

        Self { gas_price }
    }
}

// Setup a global static for accessing importer metrics
static PRODUCER_METRICS: OnceLock<ProducerMetrics> = OnceLock::new();

pub fn producer_metrics() -> &'static ProducerMetrics {
    PRODUCER_METRICS.get_or_init(ProducerMetrics::default)
}
