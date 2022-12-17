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

    if let Option::Some(value) = P2P_METRICS.gossip_sub_registry.get() {
        if encode(&mut encoded, value).is_err() {
            return Response::builder()
                .status(503)
                .body(Body::from(""))
                .unwrap()
        }
    }
    if encode(&mut encoded, &P2P_METRICS.peer_metrics).is_err() {
        return Response::builder()
            .status(503)
            .body(Body::from(""))
            .unwrap()
    }
    if encode(&mut encoded, &TXPOOL_METRICS.registry).is_err() {
        return Response::builder()
            .status(503)
            .body(Body::from(""))
            .unwrap()
    }

    Response::builder()
        .status(200)
        .body(Body::from(encoded))
        .unwrap()
}
