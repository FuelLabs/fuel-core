use lazy_static::lazy_static;
use libp2p_prom_client::{
    metrics::counter::Counter,
    registry::Registry,
};
use once_cell::race::OnceBox;

pub struct P2PMetrics {
    pub gossip_sub_registry: OnceBox<Registry>,
    // For descriptions of each Counter, see the `new` function where each Counter/Histogram is initialized
    pub peer_metrics: Registry,
    pub unique_peers: Counter,
    pub run_tracker: Counter,
}

impl P2PMetrics {
    fn new() -> Self {
        let peer_metrics = Registry::default();
        let unique_peers = Counter::default();
        let run_tracker = Counter::default();

        let mut metrics = P2PMetrics {
            gossip_sub_registry: OnceBox::new(),
            peer_metrics,
            unique_peers,
            run_tracker,
        };

        metrics.peer_metrics.register(
            "Peer_Counter",
            "A Counter which keeps track of each unique peer the p2p service has connected to",
            Box::new(metrics.unique_peers.clone()),
        );

        metrics.peer_metrics.register(
            "p2p_run_method_duration",
            "Measure time for p2p service",
            Box::new(metrics.run_tracker.clone()),
        );

        metrics
    }
}

lazy_static! {
    pub static ref P2P_METRICS: P2PMetrics = P2PMetrics::new();
}
