use lazy_static::lazy_static;
use prometheus::{self, Encoder, IntCounter, TextEncoder};

use prometheus::register_int_counter;

use crate::service::Config;
use anyhow::Result;
use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
};
use hyper::{Body, Method, Request, Response, Server};
use std::net::TcpListener;
use std::{convert::Infallible, net::SocketAddr};
use tokio::task::JoinHandle;
use tracing::info;

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

async fn metrics(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let response = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
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
        _ => Response::builder()
            .status(404)
            .body(Body::from("Not Found"))
            .unwrap(),
    };

    Ok(response)
}

pub async fn start_metrics_server(config: Config) -> Result<(SocketAddr, JoinHandle<Result<()>>)> {
    #[cfg(feature = "prometheus")]
    let listener = TcpListener::bind(&config.metrics_addr)?;
    let bound_addr = listener.local_addr().unwrap();

    let return_bound_addr = bound_addr;

    let handle = tokio::spawn(async move {
        // For every connection, we must make a `Service` to handle all
        // incoming HTTP requests on said connection.
        let make_svc = make_service_fn(move |_conn| {
            // This is the `Service` that will handle the connection.
            // `service_fn` is a helper to convert a function that
            // returns a Response into a `Service`.
            async move { Ok::<_, Infallible>(service_fn(metrics)) }
        });

        let server = Server::bind(&bound_addr).serve(make_svc);

        info!("Serving prometheus metrics on http://{}", bound_addr);

        server.await.map_err(Into::into)
    });

    Ok((return_bound_addr, handle))
}
