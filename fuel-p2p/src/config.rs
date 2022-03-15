use libp2p::{Multiaddr, PeerId};
use std::{net::IpAddr, time::Duration};

#[derive(Clone, Debug)]
pub struct P2PConfig {
    /// Name of the Network
    pub network_name: String,

    /// IP address for Swarm to listen on
    pub address: IpAddr,

    /// The TCP port that Swarm listens on
    pub tcp_port: u16,

    // `DiscoveryBehaviour` related fields
    pub bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    pub enable_mdns: bool,
    pub max_peers_connected: u64,
    pub allow_private_addresses: bool,
    pub enable_random_walk: bool,
    pub connection_idle_timeout: Option<Duration>,

    // `Gossipsub` related fields
    pub topics: Vec<String>,
    pub ideal_mesh_size: usize,
    pub min_mesh_size: usize,
    pub max_mesh_size: usize,
}
