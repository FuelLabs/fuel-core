use lazy_static::lazy_static;
use prometheus::{self, Encoder, IntCounter, TextEncoder};

use axum::response::IntoResponse;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Request, Response};
use prometheus::register_int_counter;

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
        write_meter: register_int_counter!("Writes", "Number of database write operations")
            .unwrap(),
        read_meter: register_int_counter!("Reads", "Number of database read operations").unwrap(),
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

pub async fn metrics(_req: Request<Body>) -> impl IntoResponse {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    response
}
