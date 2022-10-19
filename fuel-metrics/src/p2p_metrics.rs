use prometheus_client::{
    encoding::text::encode,
    registry::Registry,
};
use std::{sync::Arc, any};
use anyhow::*;
use lazy_static::lazy_static;
use axum::{
    body::Body,
    http::header::CONTENT_TYPE,
    response::{
        IntoResponse,
        Response,
    },
};
use std::sync::RwLock;

#[derive(Default)]
pub struct P2PMetrics {
    pub gossip_sub_registry: Registry
}

lazy_static! {
    pub static ref P2P_METRICS: RwLock<P2PMetrics> = Default::default();
}