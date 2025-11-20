use crate::global_registry;
use prometheus_client::metrics::{counter::Counter, gauge::Gauge};
use std::sync::OnceLock;

pub struct P2PMetrics {
    pub unique_peers: Counter,
    pub blocks_requested: Gauge,
    pub p2p_req_res_cache_hits: Counter,
    pub p2p_req_res_cache_misses: Counter,
}

impl P2PMetrics {
    fn new() -> Self {
        let unique_peers = Counter::default();
        let blocks_requested = Gauge::default();
        let p2p_req_res_cache_hits = Counter::default();
        let p2p_req_res_cache_misses = Counter::default();

        let metrics = P2PMetrics {
            unique_peers,
            blocks_requested,
            p2p_req_res_cache_hits,
            p2p_req_res_cache_misses,
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
