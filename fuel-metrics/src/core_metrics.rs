use axum::{
    body::Body,
    http::header::CONTENT_TYPE,
    response::{
        IntoResponse,
        Response,
    },
};
use lazy_static::lazy_static;
use prometheus::{
    self,
    register_int_counter,
    Encoder,
    IntCounter,
    TextEncoder,
};

/// DatabaseMetrics is a wrapper struct for all
/// of the initialized counters for Database-related metrics
#[derive(Clone, Debug)]
pub struct DatabaseMetrics {
    pub write_meter: IntCounter,
    pub read_meter: IntCounter,
    pub bytes_written_meter: IntCounter,
    pub bytes_read_meter: IntCounter,
}

lazy_static! {
    pub static ref DATABASE_METRICS: DatabaseMetrics = DatabaseMetrics {
        write_meter: register_int_counter!(
            "Writes",
            "Number of database write operations"
        )
        .unwrap(),
        read_meter: register_int_counter!("Reads", "Number of database read operations")
            .unwrap(),
        bytes_written_meter: register_int_counter!(
            "Bytes_Written",
            "The number of bytes written to the database"
        )
        .unwrap(),
        bytes_read_meter: register_int_counter!(
            "Bytes_Read",
            "The number of bytes read from the database"
        )
        .unwrap(),
    };
}

/// CoreMetrics tracks metrics for most of fuel-core, wrapping core metrics similar to
/// DatabaseMetrics, kept seperate from GQLMetrics which holds more specific endpoint metrics
#[derive(Clone, Debug)]
pub struct CoreMetrics {
    pub blocks_processed: IntCounter,
    pub transactions_executed: IntCounter,
    pub requests_handled: IntCounter,
}

lazy_static! {
    pub static ref CORE_METRICS: CoreMetrics = CoreMetrics {
        blocks_processed: register_int_counter!(
            "Blocks_Processed",
            "Number of blocks processed"
        )
        .unwrap(),
        transactions_executed: register_int_counter!(
            "Transactions_Processed",
            "Number of transactions executed"
        )
        .unwrap(),
        requests_handled: register_int_counter!(
            "Requests_Handled",
            "Number of GraphQL Requests handled"
        )
        .unwrap(),
    };
}

pub fn encode_metrics_response() -> impl IntoResponse {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap()
}
