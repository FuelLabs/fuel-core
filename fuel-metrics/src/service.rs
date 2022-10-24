use crate::{
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
use prometheus::{
    Encoder,
    TextEncoder,
};
use prometheus_client::encoding::text::encode;
use std::vec;

pub fn encode_metrics_response() -> impl IntoResponse {
    let mut encoded = vec![];

    // Ugly but encode database metrics with original prometheus encoder for now
    let metric_families = prometheus::gather();
    let encoder = TextEncoder::new();
    encoder.encode(&metric_families, &mut encoded).unwrap();

    encode(
        &mut encoded,
        &P2P_METRICS.read().unwrap().gossip_sub_registry,
    )
    .unwrap();
    encode(&mut encoded, &P2P_METRICS.read().unwrap().peer_metrics).unwrap();
    encode(&mut encoded, &TXPOOL_METRICS.read().unwrap().registry).unwrap();
    encode(&mut encoded, &TXPOOL_METRICS.read().unwrap().registry).unwrap();

    Response::builder()
        .status(200)
        .body(Body::from(encoded))
        .unwrap()
}
