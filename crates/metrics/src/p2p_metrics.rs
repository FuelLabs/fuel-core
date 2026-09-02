use crate::global_registry;
use prometheus_client::metrics::{counter::Counter, gauge::Gauge};
use std::sync::OnceLock;

pub struct P2PMetrics {
    pub unique_peers: Counter,
    pub blocks_requested: Gauge,
    pub p2p_req_res_cache_hits: Counter,
    pub p2p_req_res_cache_misses: Counter,
    pub functional_peers_connected: Gauge,
    pub reserved_peers_connected: Gauge,
}

impl P2PMetrics {
    fn new() -> Self {
        let unique_peers = Counter::default();
        let blocks_requested = Gauge::default();
        let p2p_req_res_cache_hits = Counter::default();
        let p2p_req_res_cache_misses = Counter::default();
        let functional_peers_connected = Gauge::default();
        let reserved_peers_connected = Gauge::default();

        let metrics = P2PMetrics {
            unique_peers,
            blocks_requested,
            p2p_req_res_cache_hits,
            p2p_req_res_cache_misses,
            functional_peers_connected,
            reserved_peers_connected,
        };

        let mut registry = global_registry().registry.lock();
        registry.register(
            "Peer_Counter",
            "A Counter which keeps track of each unique peer the p2p service has connected to",
            metrics.unique_peers.clone(),
        );

        registry.register(
            "Blocks_Requested",
            "A Gauge which keeps track of how many blocks were requested and served over the p2p req/res protocol",
            metrics.blocks_requested.clone()
        );

        registry.register(
            "P2p_Req_Res_Cache_Hits",
            "A Counter which keeps track of the number of cache hits for the p2p req/res protocol",
            metrics.p2p_req_res_cache_hits.clone()
        );

        registry.register(
            "P2p_Req_Res_Cache_Misses",
            "A Counter which keeps track of the number of cache misses for the p2p req/res protocol",
            metrics.p2p_req_res_cache_misses.clone()
        );

        registry.register(
            "Functional_Peers_Connected",
            "Current number of functional peers connected according to PeerManager. \
             This reflects application-level peers that have passed all connection filters \
             (LimitedBehaviour, max_functional_peers_connected). May differ from libp2p's \
             peer_count_per_protocol which counts at transport level.",
            metrics.functional_peers_connected.clone()
        );

        registry.register(
            "Reserved_Peers_Connected",
            "Current number of reserved peers connected. Reserved peers are not subject \
             to the functional peer limit and always have a connection slot available.",
            metrics.reserved_peers_connected.clone(),
        );

        metrics
    }
}

static P2P_METRICS: OnceLock<P2PMetrics> = OnceLock::new();

pub fn p2p_metrics() -> &'static P2PMetrics {
    P2P_METRICS.get_or_init(P2PMetrics::new)
}

pub fn increment_unique_peers() {
    p2p_metrics().unique_peers.inc();
}

pub fn set_blocks_requested(count: usize) {
    p2p_metrics().blocks_requested.set(count as i64);
}

pub fn increment_p2p_req_res_cache_hits() {
    p2p_metrics().p2p_req_res_cache_hits.inc();
}

pub fn increment_p2p_req_res_cache_misses() {
    p2p_metrics().p2p_req_res_cache_misses.inc();
}

pub fn set_functional_peers_connected(count: usize) {
    p2p_metrics().functional_peers_connected.set(count as i64);
}

pub fn set_reserved_peers_connected(count: usize) {
    p2p_metrics().reserved_peers_connected.set(count as i64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_metrics;

    #[test]
    fn functional_peers_metric_appears_in_prometheus_output() {
        set_functional_peers_connected(5);
        set_reserved_peers_connected(2);

        let output = encode_metrics().unwrap();

        assert!(
            output.contains("Functional_Peers_Connected"),
            "Metric Functional_Peers_Connected not found in output"
        );
        assert!(
            output.contains("Reserved_Peers_Connected"),
            "Metric Reserved_Peers_Connected not found in output"
        );
    }

    #[test]
    fn functional_peers_metric_updates_correctly() {
        set_functional_peers_connected(0);
        assert_eq!(p2p_metrics().functional_peers_connected.get(), 0);

        set_functional_peers_connected(10);
        assert_eq!(p2p_metrics().functional_peers_connected.get(), 10);

        set_functional_peers_connected(3);
        assert_eq!(p2p_metrics().functional_peers_connected.get(), 3);
    }

    #[test]
    fn reserved_peers_metric_updates_correctly() {
        set_reserved_peers_connected(0);
        assert_eq!(p2p_metrics().reserved_peers_connected.get(), 0);

        set_reserved_peers_connected(5);
        assert_eq!(p2p_metrics().reserved_peers_connected.get(), 5);
    }
}
