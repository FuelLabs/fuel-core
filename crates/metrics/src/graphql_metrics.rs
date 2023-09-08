use crate::timing_buckets;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        family::Family,
        histogram::Histogram,
    },
    registry::Registry,
};
use std::sync::OnceLock;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct Label {
    // the graphql path
    path: String,
}

pub struct GraphqlMetrics {
    pub registry: Registry,
    requests: Family<Label, Histogram>,
}

impl GraphqlMetrics {
    fn new() -> Self {
        let mut registry = Registry::default();
        let requests = Family::<Label, Histogram>::new_with_constructor(|| {
            Histogram::new(timing_buckets().iter().cloned())
        });
        registry.register("graphql_request_duration_seconds", "", requests.clone());
        Self { registry, requests }
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
