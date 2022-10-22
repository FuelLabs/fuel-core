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
use prometheus_client::encoding::text::encode;
use std::vec;

pub fn encode_metrics_response() -> impl IntoResponse {
    let mut encoded = vec![];
    encode(
        &mut encoded,
        &P2P_METRICS.read().unwrap().gossip_sub_registry,
    )
    .unwrap();
    encode(&mut encoded, &TXPOOL_METRICS.read().unwrap().registry).unwrap();
    Response::builder()
        .status(200)
        .body(Body::from(encoded))
        .unwrap()
}
