pub use prometheus::core::Collector;
pub use prometheus::{
    labels, Counter, CounterVec, Error as PrometheusError, Gauge, GaugeVec, Histogram,
    HistogramOpts, HistogramVec, Opts, Registry,
};

pub mod registry;
pub mod subgraph;

pub use registry::MetricsRegistry;

use std::collections::HashMap;

/// Metrics for measuring where time is spent during indexing.
pub mod stopwatch;

pub mod gas;

/// Create an unregistered counter with labels
pub fn counter_with_labels(
    name: &str,
    help: &str,
    const_labels: HashMap<String, String>,
) -> Result<Counter, PrometheusError> {
    let opts = Opts::new(name, help).const_labels(const_labels);
    Counter::with_opts(opts)
}

/// Create an unregistered gauge with labels
pub fn gauge_with_labels(
    name: &str,
    help: &str,
    const_labels: HashMap<String, String>,
) -> Result<Gauge, PrometheusError> {
    let opts = Opts::new(name, help).const_labels(const_labels);
    Gauge::with_opts(opts)
}
