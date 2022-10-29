use lazy_static::lazy_static;
use prometheus_client::{
    metrics::{
        counter::Counter,
        histogram::Histogram,
    },
    registry::Registry,
};
use std::default::Default;

pub struct DatabaseMetrics {
    pub registry: Registry,
    pub write_meter: Counter,
    pub read_meter: Counter,
    pub bytes_written: Histogram,
    pub bytes_read: Histogram,
}

impl Default for DatabaseMetrics {
    fn default() -> Self {
        let registry = Registry::default();

        let write_meter: Counter = Counter::default();
        let read_meter: Counter = Counter::default();

        let bytes_written = Vec::new();
        let bytes_written_histogram = Histogram::new(bytes_written.into_iter());

        let bytes_read = Vec::new();
        let bytes_read_histogram = Histogram::new(bytes_read.into_iter());

        DatabaseMetrics {
            registry,
            write_meter,
            read_meter,
            bytes_read: bytes_read_histogram,
            bytes_written: bytes_written_histogram,
        }
    }
}

pub fn init(mut metrics: DatabaseMetrics) -> DatabaseMetrics {
    metrics
        .registry
        .register("", "", Box::new(metrics.write_meter.clone()));
    metrics
        .registry
        .register("", "", Box::new(metrics.read_meter.clone()));
    metrics
        .registry
        .register("", "", Box::new(metrics.bytes_read.clone()));
    metrics
        .registry
        .register("", "", Box::new(metrics.bytes_written.clone()));

    metrics
}

lazy_static! {
    pub static ref DATABASE_METRICS: DatabaseMetrics = {
        let registry = DatabaseMetrics::default();

        init(registry)
    };
}
