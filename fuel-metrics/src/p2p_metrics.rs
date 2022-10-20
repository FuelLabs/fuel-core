use prometheus_client::{
    registry::Registry,
};
use lazy_static::lazy_static;
use std::sync::RwLock;

#[derive(Default)]
pub struct P2PMetrics {
    pub gossip_sub_registry: Registry
}


lazy_static! {
    pub static ref P2P_METRICS: RwLock<P2PMetrics> = Default::default();
}