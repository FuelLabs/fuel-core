use crate::{
    buckets::{
        buckets,
        Buckets,
    },
    global_registry,
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
    queries_complexity: Histogram,
}

impl GraphqlMetrics {
    fn new() -> Self {
        let tx_count_gauge = Gauge::default();
        let queries_complexity = Histogram::new(buckets_complexity());
        let requests = Family::<Label, Histogram>::new_with_constructor(|| {
            Histogram::new(buckets(Buckets::Timing))
        });
        let mut registry = global_registry().registry.lock();
        registry.register("graphql_request_duration_seconds", "", requests.clone());
        registry.register(
            "graphql_query_complexity",
            "The complexity of all queries received",
            queries_complexity.clone(),
        );

        registry.register(
            "importer_tx_count",
            "the total amount of transactions that have been imported on chain",
            tx_count_gauge.clone(),
        );

        Self {
            total_txs_count: tx_count_gauge,
            queries_complexity,
            requests,
        }
    }

    pub fn graphql_observe(&self, query: &str, time: f64) {
        let histogram = self.requests.get_or_create(&Label {
            path: query.to_string(),
        });
        histogram.observe(time);
    }

    pub fn graphql_complexity_observe(&self, complexity: f64) {
        self.queries_complexity.observe(complexity);
    }
}

static GRAPHQL_METRICS: OnceLock<GraphqlMetrics> = OnceLock::new();
pub fn graphql_metrics() -> &'static GraphqlMetrics {
    GRAPHQL_METRICS.get_or_init(GraphqlMetrics::new)
}

fn buckets_complexity() -> impl Iterator<Item = f64> {
    [
        1_000.0,
        5_000.0,
        10_000.0,
        20_000.0,
        50_000.0,
        100_000.0,
        250_000.0,
        500_000.0,
        1_000_000.0,
        5_000_000.0,
        10_000_000.0,
    ]
    .into_iter()
}
