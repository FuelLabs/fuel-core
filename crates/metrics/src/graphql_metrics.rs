use lazy_static::lazy_static;
use prometheus_client::{
    metrics::{
        counter::Counter,
        histogram::Histogram,
    },
    registry::Registry,
};

pub struct GraphqlMetrics {
    pub registry: Registry,
    pub num_of_requests: Counter,
    pub response_times: Histogram,
}

impl GraphqlMetrics {
    fn new() -> Self {
        let registry = Registry::default();

        let response_bytes = Vec::new();
        let response_times = Histogram::new(response_bytes.into_iter());

        let num_of_requests = Counter::default();

        let mut metrics = Self {
            registry,
            response_times,
            num_of_requests,
        };

        metrics.registry.register(
            "Number_of_Requests",
            "Count of number of requests per second",
            Box::new(metrics.num_of_requests.clone()),
        );

        metrics.registry.register(
            "Response_Times",
            "Histogram containing values of response times of graphql queries",
            Box::new(metrics.response_times.clone()),
        );

        metrics
    }
}

lazy_static! {
    pub static ref GRAPHQL_METRICS: GraphqlMetrics = GraphqlMetrics::new();
}
