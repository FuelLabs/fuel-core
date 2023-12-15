use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;

use anyhow::Error;
use hyper::header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Response, Server};
use thiserror::Error;

use graph::prelude::*;
use graph::prometheus::{Encoder, Registry, TextEncoder};

/// Errors that may occur when starting the server.
#[derive(Debug, Error)]
pub enum PrometheusMetricsServeError {
    #[error("Bind error: {0}")]
    BindError(#[from] hyper::Error),
}

#[derive(Clone)]
pub struct PrometheusMetricsServer {
    logger: Logger,
    registry: Arc<Registry>,
}

impl PrometheusMetricsServer {
    pub fn new(logger_factory: &LoggerFactory, registry: Arc<Registry>) -> Self {
        PrometheusMetricsServer {
            logger: logger_factory.component_logger("MetricsServer", None),
            registry,
        }
    }

    /// Creates a new Tokio task that, when spawned, brings up the index node server.
    pub async fn serve(
        &mut self,
        port: u16,
    ) -> Result<Result<(), ()>, PrometheusMetricsServeError> {
        let logger = self.logger.clone();

        info!(
            logger,
            "Starting metrics server at: http://localhost:{}", port,
        );

        let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port);

        let server = self.clone();
        let new_service = make_service_fn(move |_req| {
            let server = server.clone();
            async move {
                Ok::<_, Error>(service_fn(move |_| {
                    let metric_families = server.registry.gather();
                    let mut buffer = vec![];
                    let encoder = TextEncoder::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();
                    futures03::future::ok::<_, Error>(
                        Response::builder()
                            .status(200)
                            .header(CONTENT_TYPE, encoder.format_type())
                            .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .body(Body::from(buffer))
                            .unwrap(),
                    )
                }))
            }
        });

        let task = Server::try_bind(&addr.into())?
            .serve(new_service)
            .map_err(move |e| error!(logger, "Metrics server error"; "error" => format!("{}", e)));

        Ok(task.await)
    }
}
