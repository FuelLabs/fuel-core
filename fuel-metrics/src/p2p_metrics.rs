use lazy_static::lazy_static;
use once_cell::race::OnceBox;
use prometheus_client::{
    metrics::counter::Counter,
    registry::Registry,
};
use std::{
    boxed::Box,
    default::Default,
};

pub struct P2PMetrics {
    pub gossip_sub_registry: OnceBox<Registry>,
    pub peer_metrics: Registry,
    pub unique_peers: Counter,
}

impl Default for P2PMetrics {
    fn default() -> Self {
        let peer_metrics = Registry::default();

        let unique_peers = Counter::default();

        Self {
            gossip_sub_registry: OnceBox::new(),
            peer_metrics,
            unique_peers,
        }
    }
}

pub fn init(mut metrics: P2PMetrics) -> P2PMetrics {
    metrics.peer_metrics.register(
        "Peer_Counter",
        "A Counter which keeps track of each unique peer the p2p service has connected to",
        Box::new(metrics.unique_peers.clone()),
    );

    metrics
}

lazy_static! {
    pub static ref P2P_METRICS: P2PMetrics = {
        let registry = P2PMetrics::default();

        init(registry)
    };
}
