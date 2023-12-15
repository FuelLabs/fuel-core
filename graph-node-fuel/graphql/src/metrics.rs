use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use graph::data::query::QueryResults;
use graph::prelude::{DeploymentHash, GraphQLMetrics as GraphQLMetricsTrait, MetricsRegistry};
use graph::prometheus::{CounterVec, Gauge, Histogram, HistogramVec};

pub struct GraphQLMetrics {
    query_execution_time: Box<HistogramVec>,
    query_parsing_time: Box<HistogramVec>,
    query_validation_time: Box<HistogramVec>,
    query_result_size: Box<Histogram>,
    query_result_size_max: Box<Gauge>,
    query_validation_error_counter: Box<CounterVec>,
    query_blocks_behind: Box<HistogramVec>,
}

impl fmt::Debug for GraphQLMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GraphQLMetrics {{ }}")
    }
}

impl GraphQLMetricsTrait for GraphQLMetrics {
    fn observe_query_execution(&self, duration: Duration, results: &QueryResults) {
        let id = results
            .deployment_hash()
            .map(|h| h.as_str())
            .unwrap_or_else(|| {
                if results.not_found() {
                    "notfound"
                } else {
                    "unknown"
                }
            });
        let status = if results.has_errors() {
            "failed"
        } else {
            "success"
        };
        self.query_execution_time
            .with_label_values(&[id, status])
            .observe(duration.as_secs_f64());
    }

    fn observe_query_parsing(&self, duration: Duration, results: &QueryResults) {
        let id = results
            .deployment_hash()
            .map(|h| h.as_str())
            .unwrap_or_else(|| {
                if results.not_found() {
                    "notfound"
                } else {
                    "unknown"
                }
            });
        self.query_parsing_time
            .with_label_values(&[id])
            .observe(duration.as_secs_f64());
    }

    fn observe_query_validation(&self, duration: Duration, id: &DeploymentHash) {
        self.query_validation_time
            .with_label_values(&[id.as_str()])
            .observe(duration.as_secs_f64());
    }

    fn observe_query_validation_error(&self, error_codes: Vec<&str>, id: &DeploymentHash) {
        for code in error_codes.iter() {
            self.query_validation_error_counter
                .with_label_values(&[id.as_str(), *code])
                .inc();
        }
    }

    fn observe_query_blocks_behind(&self, blocks_behind: i32, id: &DeploymentHash) {
        self.query_blocks_behind
            .with_label_values(&[id.as_str()])
            .observe(blocks_behind as f64);
    }
}

impl GraphQLMetrics {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        let query_execution_time = registry
            .new_histogram_vec(
                "query_execution_time",
                "Execution time for successful GraphQL queries",
                vec![String::from("deployment"), String::from("status")],
                vec![0.1, 0.5, 1.0, 10.0, 100.0],
            )
            .expect("failed to create `query_execution_time` histogram");
        let query_parsing_time = registry
            .new_histogram_vec(
                "query_parsing_time",
                "Parsing time for GraphQL queries",
                vec![String::from("deployment")],
                vec![0.1, 0.5, 1.0, 10.0, 100.0],
            )
            .expect("failed to create `query_parsing_time` histogram");

        let query_validation_time = registry
            .new_histogram_vec(
                "query_validation_time",
                "Validation time for GraphQL queries",
                vec![String::from("deployment")],
                vec![0.1, 0.5, 1.0, 10.0, 100.0],
            )
            .expect("failed to create `query_validation_time` histogram");

        let bins = (10..32).map(|n| 2u64.pow(n) as f64).collect::<Vec<_>>();
        let query_result_size = registry
            .new_histogram(
                "query_result_size",
                "the size of the result of successful GraphQL queries (in CacheWeight)",
                bins,
            )
            .unwrap();

        let query_result_size_max = registry
            .new_gauge(
                "query_result_max",
                "the maximum size of a query result (in CacheWeight)",
                HashMap::new(),
            )
            .unwrap();

        let query_validation_error_counter = registry
            .new_counter_vec(
                "query_validation_error_counter",
                "a counter for the number of validation errors",
                vec![String::from("deployment"), String::from("error_code")],
            )
            .unwrap();

        let query_blocks_behind = registry
            .new_histogram_vec(
                "query_blocks_behind",
                "How many blocks the query block is behind the subgraph head",
                vec![String::from("deployment")],
                vec![
                    0.0, 5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 10000.0,
                    100000.0, 1000000.0, 10000000.0,
                ],
            )
            .unwrap();

        Self {
            query_execution_time,
            query_parsing_time,
            query_validation_time,
            query_result_size,
            query_result_size_max,
            query_validation_error_counter,
            query_blocks_behind,
        }
    }

    // Tests need to construct one of these, but normal code doesn't
    #[cfg(debug_assertions)]
    pub fn make(registry: Arc<MetricsRegistry>) -> Self {
        Self::new(registry)
    }

    pub fn observe_query_result_size(&self, size: usize) {
        let size = size as f64;
        self.query_result_size.observe(size);
        if self.query_result_size_max.get() < size {
            self.query_result_size_max.set(size);
        }
    }
}
