use crate::discovery::{mdns::MdnsWrapper, DiscoveryBehaviour};
use futures_timer::Delay;
use libp2p::{
    kad::{store::MemoryStore, Kademlia, KademliaConfig},
    Multiaddr, PeerId,
};
use std::{collections::VecDeque, time::Duration};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    local_peer_id: PeerId,
    bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    with_mdns: bool,
    with_random_walk: bool,
    allow_private_addresses: bool,
    network_name: String,
    max_peers_connected: u64,
    connection_idle_timeout: Duration,
}

impl DiscoveryConfig {
    pub fn new(local_peer_id: PeerId, network_name: String) -> Self {
        Self {
            local_peer_id,
            bootstrap_nodes: vec![],
            max_peers_connected: std::u64::MAX,
            allow_private_addresses: false,
            with_mdns: false,
            network_name,
            with_random_walk: false,
            connection_idle_timeout: Duration::from_secs(10),
        }
    }

    /// limit the number of connected nodes
    pub fn discovery_limit(&mut self, limit: u64) -> &mut Self {
        self.max_peers_connected = limit;
        self
    }

    /// Enable reporting of private addresses
    pub fn allow_private_addresses(&mut self, value: bool) -> &mut Self {
        self.allow_private_addresses = value;
        self
    }

    /// Sets the amount of time to keep connections alive when they're idle
    pub fn set_connection_idle_timeout(&mut self, connection_idle_timeout: Duration) -> &mut Self {
        self.connection_idle_timeout = connection_idle_timeout;
        self
    }

    // List of bootstrap nodes to bootstrap the network
    pub fn with_bootstrap_nodes<I>(&mut self, bootstrap_nodes: I) -> &mut Self
    where
        I: IntoIterator<Item = (PeerId, Multiaddr)>,
    {
        self.bootstrap_nodes.extend(bootstrap_nodes);
        self
    }

    pub fn enable_mdns(&mut self, value: bool) -> &mut Self {
        self.with_mdns = value;
        self
    }

    pub fn enable_random_walk(&mut self, value: bool) -> &mut Self {
        self.with_random_walk = value;
        self
    }

    pub fn finish(self) -> DiscoveryBehaviour {
        let DiscoveryConfig {
            local_peer_id,
            bootstrap_nodes,
            network_name,
            max_peers_connected,
            allow_private_addresses,
            connection_idle_timeout,
            ..
        } = self;

        // kademlia setup
        let memory_store = MemoryStore::new(local_peer_id.to_owned());
        let mut kademlia_config = KademliaConfig::default();
        let network = format!("/fuel/kad/{}/kad/1.0.0", network_name);
        kademlia_config.set_protocol_name(network.as_bytes().to_vec());
        kademlia_config.set_connection_idle_timeout(connection_idle_timeout);
        let mut kademlia = Kademlia::with_config(local_peer_id, memory_store, kademlia_config);

        for (peer_id, addr) in &bootstrap_nodes {
            kademlia.add_address(peer_id, addr.clone());
        }

        if let Err(e) = kademlia.bootstrap() {
            warn!("Kademlia bootstrap failed: {}", e);
        }

        let next_kad_random_walk = if self.with_random_walk {
            Some(Delay::new(Duration::new(0, 0)))
        } else {
            None
        };

        // mdns setup
        let mdns = if self.with_mdns {
            MdnsWrapper::new()
        } else {
            MdnsWrapper::disabled()
        };

        DiscoveryBehaviour {
            bootstrap_nodes,
            events: VecDeque::new(),
            kademlia,
            next_kad_random_walk,
            duration_to_next_kad: Duration::from_secs(1),
            connected_peers_count: 0,
            max_peers_connected,
            mdns,
            allow_private_addresses,
        }
    }
}
