use crate::{
    graphql_metrics::graphql_metrics,
    importer::importer_metrics,
    p2p_metrics::p2p_metrics,
    services::services_metrics,
    txpool_metrics::txpool_metrics,
};
use axum::{
    body::Body,
    response::{
        IntoResponse,
        Response,
    },
};
use core::ops::Deref;
use prometheus_client::encoding::text::encode;

pub fn encode_metrics_response() -> impl IntoResponse {
    // encode libp2p metrics using older prometheus
    let mut encoded = String::new();
    if let Some(value) = p2p_metrics().gossip_sub_registry.get() {
        if encode(&mut encoded, value).is_err() {
            return error_body()
        }
    }
    if encode(&mut encoded, &p2p_metrics().peer_metrics).is_err() {
        return error_body()
    }

    // encode the rest of the fuel-core metrics using latest prometheus
    {
        let lock = services_metrics()
            .registry
            .lock()
            .expect("The service metrics lock is poisoned");
        if encode(&mut encoded, lock.deref()).is_err() {
            return error_body()
        }
    }

    if encode(&mut encoded, &txpool_metrics().registry).is_err() {
        return error_body()
    }

    if encode(&mut encoded, &graphql_metrics().registry).is_err() {
        return error_body()
    }

    if encode(&mut encoded, &importer_metrics().registry).is_err() {
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
