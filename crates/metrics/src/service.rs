use std::time::Instant;

use crate::{
    graphql_metrics::GRAPHQL_METRICS,
    p2p_metrics::P2P_METRICS,
    txpool_metrics::TXPOOL_METRICS,
};
use axum::{
    body::Body,
    response::{
        IntoResponse,
        Response,
    },
};
use libp2p_prom_client::encoding::text::encode as libp2p_encode;
use prometheus_client::{
    encoding::text::encode,
    metrics::counter::Counter,
    registry::Registry,
};

pub fn encode_metrics_response() -> impl IntoResponse {
    // encode libp2p metrics using older prometheus
    let mut libp2p_bytes = Vec::<u8>::new();
    if let Some(value) = P2P_METRICS.gossip_sub_registry.get() {
        if libp2p_encode(&mut libp2p_bytes, value).is_err() {
            return error_body()
        }
    }
    if libp2p_encode(&mut libp2p_bytes, &P2P_METRICS.peer_metrics).is_err() {
        return error_body()
    }

    let mut encoded = String::from_utf8_lossy(&libp2p_bytes).into_owned();

    // encode the rest of the fuel-core metrics using latest prometheus
    if encode(&mut encoded, &TXPOOL_METRICS.registry).is_err() {
        return error_body()
    }

    if encode(&mut encoded, &GRAPHQL_METRICS.registry).is_err() {
        return error_body()
    }

    Response::builder()
        .status(200)
        .body(Body::from(encoded))
        .unwrap()
}

fn error_body() -> Response<Body> {
    Response::builder()
        .status(503)
        .body(Body::from(""))
        .unwrap()
}

pub struct ServiceMetrics {
    pub registry: Registry,
    pub run_tracker: Counter,
}

impl ServiceMetrics {
    pub fn new(name: &str) -> Self {
        let registry = Registry::default();

        let run_tracker = Counter::default();

        let mut metrics = ServiceMetrics {
            registry,
            run_tracker,
        };

        metrics.registry.register(
            name,
            "Measure time for service's run() method",
            metrics.run_tracker.clone(),
        );

        metrics
    }

    pub fn instant() -> Instant {
        Instant::now()
    }

    pub fn observe(&self, start: Instant) {
        self.run_tracker.inc_by(start.elapsed().as_secs());
    }
}

// lazy_static! {
//     pub static ref TXPOOL_METRICS: TxPoolMetrics = TxPoolMetrics::default();
// }
