use prometheus_client::{
    metrics::{
        counter::Counter,
        histogram::Histogram,
    },
    registry::Registry,
};
use std::sync::OnceLock;

pub struct DatabaseMetrics {
    pub registry: Registry,
    // For descriptions of each Counter, see the `new` function where each Counter/Histogram is initialized
    pub write_meter: Counter,
    pub read_meter: Counter,
    pub bytes_written: Histogram,
    pub bytes_read: Histogram,
}

impl DatabaseMetrics {
    fn new() -> Self {
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
    metrics.registry.register(
        "Database_Writes",
        "Number of database write operations",
        metrics.write_meter.clone(),
    );
    metrics.registry.register(
        "Database_Reads",
        "Number of database read operations",
        metrics.read_meter.clone(),
    );
    metrics.registry.register(
        "Bytes_Read",
        "Histogram containing values of amount of bytes read per operation",
        metrics.bytes_read.clone(),
    );
    metrics.registry.register(
        "Bytes_Written",
        "Histogram containing values of amount of bytes written per operation",
        metrics.bytes_written.clone(),
    );

    metrics
}

static DATABASE_METRICS: OnceLock<DatabaseMetrics> = OnceLock::new();

pub fn database_metrics() -> &'static DatabaseMetrics {
    DATABASE_METRICS.get_or_init(|| {
        let registry = DatabaseMetrics::new();
        init(registry)
    })
}
