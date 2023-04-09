use lazy_static::lazy_static;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        family::Family,
        histogram::Histogram,
    },
    registry::Registry,
};

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
            Histogram::new(BUCKETS.iter().cloned())
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

lazy_static! {
    pub static ref GRAPHQL_METRICS: GraphqlMetrics = GraphqlMetrics::new();
    // recommended bucket defaults for API response times
    static ref BUCKETS: Vec<f64> =
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];
}
