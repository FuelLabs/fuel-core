use crate::global_registry;
use prometheus_client::metrics::counter::Counter;
use std::sync::OnceLock;

pub struct P2PMetrics {
    pub unique_peers: Counter,
}

impl P2PMetrics {
    fn new() -> Self {
        let unique_peers = Counter::default();

        let metrics = P2PMetrics { unique_peers };

        let mut registry = global_registry().registry.lock();
        registry.register(
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
