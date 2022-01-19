use std::{collections::VecDeque, time::Duration};

use crate::discovery::{mdns::MdnsWorker, Discovery};
use futures_timer::Delay;
use libp2p::{
    core::PublicKey,
    kad::{store::MemoryStore, Kademlia, KademliaConfig},
    multiaddr::Protocol,
    Multiaddr, PeerId,
};
use log::warn;
pub struct DiscoveryConfig {
    local_peer_id: PeerId,
    predefined_nodes: Vec<(PeerId, Multiaddr)>,
    with_mdns: bool,
    with_random_walk: bool,
    network_name: String,
    max_peers_connected: u64,
}

impl DiscoveryConfig {
    pub fn new(public_key: PublicKey, network_name: String) -> Self {
        Self {
            local_peer_id: public_key.to_peer_id(),
            predefined_nodes: vec![],
            max_peers_connected: std::u64::MAX,
            with_mdns: false,
            network_name,
            with_random_walk: false,
        }
    }

    // limit the number of connected nodes
    pub fn discovery_limit(&mut self, limit: u64) -> &mut Self {
        self.max_peers_connected = limit;
        self
    }

    // bootstrapped nodes
    pub fn with_predefined_nodes<I>(&mut self, predefined_nodes: I) -> &mut Self
    where
        I: IntoIterator<Item = Multiaddr>,
    {
        let correct_predefined_nodes = predefined_nodes.into_iter().filter_map(|mut multiaddr| {
            if let Some(Protocol::P2p(multi_hash)) = multiaddr.pop() {
                let peer_id = PeerId::from_multihash(multi_hash).unwrap();
                Some((peer_id, multiaddr))
            } else {
                warn!("Failed to parse predefined node: {}", multiaddr);
                None
            }
        });

        self.predefined_nodes.extend(correct_predefined_nodes);
        self
    }

    pub fn enable_mdns(&mut self) -> &mut Self {
        self.with_mdns = true;
        self
    }

    pub fn enable_random_walk(&mut self) -> &mut Self {
        self.with_random_walk = true;
        self
    }

    pub fn finish(self) -> Discovery {
        let DiscoveryConfig {
            local_peer_id: peer_id,
            predefined_nodes,
            network_name,
            ..
        } = self;

        // kademlia setup
        let memory_store = MemoryStore::new(peer_id.to_owned());
        let mut kademlia_config = KademliaConfig::default();
        let network = format!("/fuel/kad/{}/kad/1.0.0", network_name);
        kademlia_config.set_protocol_name(network.as_bytes().to_vec());
        let mut kademlia = Kademlia::with_config(peer_id, memory_store, kademlia_config);

        for (peer_id, addr) in &predefined_nodes {
            kademlia.add_address(peer_id, addr.clone());
        }

        if let Err(e) = kademlia.bootstrap() {
            warn!("Kademlia bootstrap failed: {}", e);
        }

        let next_kad_random_query = if self.with_random_walk {
            Some(Delay::new(Duration::new(0, 0)))
        } else {
            None
        };

        // mdns setup
        let mdns = if self.with_mdns {
            MdnsWorker::new()
        } else {
            MdnsWorker::disabled()
        };

        Discovery {
            local_peer_id: self.local_peer_id,
            predefined_nodes,
            events: VecDeque::new(),

            kademlia,
            next_kad_random_query,
            duration_to_next_kad: Duration::from_secs(1),
            connected_peers_count: 0,
            max_peers_connected: self.max_peers_connected,

            mdns,
        }
    }
}
