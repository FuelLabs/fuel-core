use crate::discovery::{
    mdns::MdnsWrapper,
    DiscoveryBehaviour,
};
use futures_timer::Delay;
use libp2p::{
    kad::{
        store::MemoryStore,
        Kademlia,
        KademliaConfig,
    },
    Multiaddr,
    PeerId,
};
use std::{
    collections::{
        HashSet,
        VecDeque,
    },
    time::Duration,
};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    local_peer_id: PeerId,
    bootstrap_nodes: Vec<Multiaddr>,
    reserved_nodes: Vec<Multiaddr>,
    reserved_nodes_only_mode: bool,
    random_walk: Option<Duration>,
    with_mdns: bool,
    allow_private_addresses: bool,
    network_name: String,
    max_peers_connected: usize,
    connection_idle_timeout: Duration,
}

impl DiscoveryConfig {
    pub fn new(local_peer_id: PeerId, network_name: String) -> Self {
        Self {
            local_peer_id,
            bootstrap_nodes: vec![],
            reserved_nodes: vec![],
            reserved_nodes_only_mode: false,
            random_walk: None,
            max_peers_connected: std::usize::MAX,
            allow_private_addresses: false,
            with_mdns: false,
            network_name,
            connection_idle_timeout: Duration::from_secs(10),
        }
    }

    /// limit the number of connected nodes
    pub fn discovery_limit(&mut self, limit: usize) -> &mut Self {
        self.max_peers_connected = limit;
        self
    }

    /// Enable reporting of private addresses
    pub fn allow_private_addresses(&mut self, value: bool) -> &mut Self {
        self.allow_private_addresses = value;
        self
    }

    /// Sets the amount of time to keep connections alive when they're idle
    pub fn set_connection_idle_timeout(
        &mut self,
        connection_idle_timeout: Duration,
    ) -> &mut Self {
        self.connection_idle_timeout = connection_idle_timeout;
        self
    }

    // List of bootstrap nodes to bootstrap the network
    pub fn with_bootstrap_nodes<I>(&mut self, bootstrap_nodes: I) -> &mut Self
    where
        I: IntoIterator<Item = Multiaddr>,
    {
        self.bootstrap_nodes.extend(bootstrap_nodes);
        self
    }

    // List of reserved nodes to bootstrap the network
    pub fn with_reserved_nodes<I>(&mut self, reserved_nodes: I) -> &mut Self
    where
        I: IntoIterator<Item = Multiaddr>,
    {
        self.reserved_nodes.extend(reserved_nodes);
        self
    }

    pub fn enable_reserved_nodes_only_mode(&mut self, value: bool) -> &mut Self {
        self.reserved_nodes_only_mode = value;
        self
    }

    pub fn enable_mdns(&mut self, value: bool) -> &mut Self {
        self.with_mdns = value;
        self
    }

    pub fn with_random_walk(&mut self, value: Duration) -> &mut Self {
        self.random_walk = Some(value);
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
            reserved_nodes,
            reserved_nodes_only_mode,
            ..
        } = self;

        // kademlia setup
        let memory_store = MemoryStore::new(local_peer_id.to_owned());
        let mut kademlia_config = KademliaConfig::default();
        let network = format!("/fuel/kad/{}/kad/1.0.0", network_name);
        let network_name = network.as_bytes().to_vec();
        kademlia_config.set_protocol_names(vec![network_name.into()]);
        kademlia_config.set_connection_idle_timeout(connection_idle_timeout);

        let mut kademlia =
            Kademlia::with_config(local_peer_id, memory_store, kademlia_config);

        // bootstrap nodes need to have their peer_id defined in the Multiaddr
        let bootstrap_nodes = bootstrap_nodes
            .into_iter()
            .filter_map(|node| {
                PeerId::try_from_multiaddr(&node).map(|peer_id| (peer_id, node))
            })
            .collect::<Vec<_>>();

        // reserved nodes need to have their peer_id defined in the Multiaddr
        let reserved_nodes = reserved_nodes
            .into_iter()
            .filter_map(|node| {
                PeerId::try_from_multiaddr(&node).map(|peer_id| (peer_id, node))
            })
            .collect::<Vec<_>>();

        // add bootstrap nodes only if `reserved_nodes_only_mode` is disabled
        if !reserved_nodes_only_mode {
            for (peer_id, address) in &bootstrap_nodes {
                kademlia.add_address(peer_id, address.clone());
            }
        }

        for (peer_id, address) in &reserved_nodes {
            kademlia.add_address(peer_id, address.clone());
        }

        if let Err(e) = kademlia.bootstrap() {
            warn!("Kademlia bootstrap failed: {}", e);
        }

        let next_kad_random_walk = {
            let random_walk = self.random_walk.map(Delay::new);

            // no need to preferm random walk if we don't want the node to connect to non-whitelisted peers
            if !reserved_nodes_only_mode {
                random_walk
            } else {
                None
            }
        };

        // mdns setup
        let mdns = if self.with_mdns {
            MdnsWrapper::default()
        } else {
            MdnsWrapper::disabled()
        };

        DiscoveryBehaviour {
            bootstrap_nodes,
            reserved_nodes,
            connected_peers: HashSet::new(),
            pending_events: VecDeque::new(),
            kademlia,
            next_kad_random_walk,
            duration_to_next_kad: Duration::from_secs(1),
            max_peers_connected,
            mdns,
            allow_private_addresses,
        }
    }
}
