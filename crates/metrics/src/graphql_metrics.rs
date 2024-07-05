use crate::{
    global_registry,
    timing_buckets,
};
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        family::Family,
        gauge::Gauge,
        histogram::Histogram,
    },
};
use std::sync::OnceLock;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Label {
    // the graphql path
    path: String,
}

pub struct GraphqlMetrics {
    // using gauges in case blocks are rolled back for any reason
    pub total_txs_count: Gauge,
    requests: Family<Label, Histogram>,
}

impl GraphqlMetrics {
    fn new() -> Self {
        let tx_count_gauge = Gauge::default();
        let requests = Family::<Label, Histogram>::new_with_constructor(|| {
            Histogram::new(timing_buckets().iter().cloned())
        });
        let mut registry = global_registry().registry.lock();
        registry.register("graphql_request_duration_seconds", "", requests.clone());

        registry.register(
            "importer_tx_count",
            "the total amount of transactions that have been imported on chain",
            tx_count_gauge.clone(),
        );

        Self {
            total_txs_count: tx_count_gauge,
            requests,
        }
    }

    pub fn graphql_observe(&self, query: &str, time: f64) {
        let histogram = self.requests.get_or_create(&Label {
            path: query.to_string(),
        });
        histogram.observe(time);
    }
}

static GRAPHQL_METRICS: OnceLock<GraphqlMetrics> = OnceLock::new();
pub fn graphql_metrics() -> &'static GraphqlMetrics {
    GRAPHQL_METRICS.get_or_init(GraphqlMetrics::new)
}
