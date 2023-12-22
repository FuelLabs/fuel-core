use once_cell::race::OnceBox;
use prometheus_client::{
    metrics::counter::Counter,
    registry::Registry,
};
use std::sync::OnceLock;

pub struct P2PMetrics {
    pub gossip_sub_registry: OnceBox<Registry>,
    // For descriptions of each Counter, see the `new` function where each Counter/Histogram is initialized
    pub peer_metrics: Registry,
    pub unique_peers: Counter,
}

impl P2PMetrics {
    fn new() -> Self {
        let peer_metrics = Registry::default();

        let unique_peers = Counter::default();

        let mut metrics = P2PMetrics {
            gossip_sub_registry: OnceBox::new(),
            peer_metrics,
            unique_peers,
        };

        metrics.peer_metrics.register(
            "Peer_Counter",
            "A Counter which keeps track of each unique peer the p2p service has connected to",
            metrics.unique_peers.clone(),
        );

        metrics
    }
}

static P2P_METRICS: OnceLock<P2PMetrics> = OnceLock::new();

pub fn p2p_metrics() -> &'static P2PMetrics {
    P2P_METRICS.get_or_init(P2PMetrics::new)
}
