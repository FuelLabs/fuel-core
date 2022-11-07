use std::{
    net::{
        IpAddr,
        Ipv4Addr,
    },
    path::PathBuf,
    time::Duration,
};

use clap::Args;

use fuel_core_interfaces::common::fuel_crypto::SecretKey;
use fuel_p2p::{
    config::P2PConfig,
    gossipsub_config::default_gossipsub_builder,
    Multiaddr,
};

#[derive(Debug, Clone, Args)]
pub struct P2pArgs {
    /// Path to the location of DER-encoded Secp256k1 Keypair
    #[clap(long = "keypair")]
    pub keypair: Option<PathBuf>,

    /// The name of the p2p Network
    /// If this value is not provided the p2p network won't start
    #[clap(long = "network", default_value = "")]
    pub network: String,

    /// p2p network's IP Address
    #[clap(long = "address")]
    pub address: Option<IpAddr>,

    /// p2p network's TCP Port
    #[clap(long = "peering-port", default_value = "4001")]
    pub peering_port: u16,

    /// Max Block size
    #[clap(long = "max_block_size", default_value = "100000")]
    pub max_block_size: usize,

    /// Addresses of the bootstrap nodes
    /// They should contain PeerId at the end of the specified Multiaddr
    #[clap(long = "bootstrap_nodes")]
    pub bootstrap_nodes: Vec<Multiaddr>,

    /// Allow nodes to be discoverable on the local network
    #[clap(long = "enable_mdns")]
    pub enable_mdns: bool,

    /// Maximum amount of allowed connected peers
    #[clap(long = "max_peers_connected", default_value = "50")]
    pub max_peers_connected: usize,

    /// Enable random walk for p2p node discovery
    #[clap(long = "enable_random_walk")]
    pub enable_random_walk: bool,

    /// Choose to include private IPv4/IPv6 addresses as discoverable
    /// except for the ones stored in `bootstrap_nodes`
    #[clap(long = "allow_private_addresses")]
    pub allow_private_addresses: bool,

    /// Choose how long will connection keep alive if idle
    #[clap(long = "connection_idle_timeout", default_value = "120")]
    pub connection_idle_timeout: u64,

    /// Choose how often to recieve PeerInfo from other nodes
    #[clap(long = "info_interval", default_value = "3")]
    pub info_interval: u64,

    /// Choose the interval at which identification requests are sent to
    /// the remote on established connections after the first request
    #[clap(long = "identify_interval", default_value = "5")]
    pub identify_interval: u64,

    /// Choose which topics to subscribe to via gossipsub protocol
    #[clap(long = "topics", default_values = &["new_tx", "new_block", "consensus_vote"])]
    pub topics: Vec<String>,

    /// Choose max mesh size for gossipsub protocol
    #[clap(long = "max_mesh_size", default_value = "12")]
    pub max_mesh_size: usize,

    /// Choose min mesh size for gossipsub protocol
    #[clap(long = "min_mesh_size", default_value = "4")]
    pub min_mesh_size: usize,

    /// Choose ideal mesh size for gossipsub protocol
    #[clap(long = "ideal_mesh_size", default_value = "6")]
    pub ideal_mesh_size: usize,

    /// Number of heartbeats to keep in the gossipsub `memcache`
    #[clap(long = "history_length", default_value = "5")]
    pub history_length: usize,

    /// Number of past heartbeats to gossip about
    #[clap(long = "history_gossip", default_value = "3")]
    pub history_gossip: usize,

    /// Time between each heartbeat
    #[clap(long = "heartbeat_interval", default_value = "1")]
    pub heartbeat_interval: u64,

    /// The maximum byte size for each gossip
    #[clap(long = "max_transmit_size", default_value = "2048")]
    pub max_transmit_size: usize,

    /// Choose timeout for sent requests in RequestResponse protocol
    #[clap(long = "request_timeout", default_value = "20")]
    pub request_timeout: u64,

    /// Choose how long RequestResponse protocol connections will live if idle
    #[clap(long = "connection_keep_alive", default_value = "20")]
    pub connection_keep_alive: u64,
}

impl From<P2pArgs> for anyhow::Result<P2PConfig> {
    fn from(args: P2pArgs) -> Self {
        let local_keypair = {
            match args.keypair {
                Some(path) => {
                    let phrase = std::fs::read_to_string(path)?;

                    let secret_key = SecretKey::new_from_mnemonic_phrase_with_path(
                        &phrase,
                        "m/44'/60'/0'/0/0",
                    )?;

                    fuel_p2p::config::convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
                _ => {
                    let mut rand =
                        fuel_core_interfaces::common::fuel_crypto::rand::thread_rng();
                    let secret_key = SecretKey::random(&mut rand);

                    fuel_p2p::config::convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
            }
        };

        let gossipsub_config = default_gossipsub_builder()
            .mesh_n(args.ideal_mesh_size)
            .mesh_n_low(args.min_mesh_size)
            .mesh_n_high(args.max_mesh_size)
            .history_length(args.history_length)
            .history_gossip(args.history_gossip)
            .heartbeat_interval(Duration::from_secs(args.heartbeat_interval))
            .max_transmit_size(args.max_transmit_size)
            .build()
            .expect("valid gossipsub configuration");

        Ok(P2PConfig {
            local_keypair,
            network_name: args.network,
            address: args
                .address
                .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0]))),
            tcp_port: args.peering_port,
            max_block_size: args.max_block_size,
            bootstrap_nodes: args.bootstrap_nodes,
            enable_mdns: args.enable_mdns,
            max_peers_connected: args.max_peers_connected,
            allow_private_addresses: args.allow_private_addresses,
            enable_random_walk: args.enable_random_walk,
            connection_idle_timeout: Some(Duration::from_secs(
                args.connection_idle_timeout,
            )),
            topics: args.topics,
            gossipsub_config,
            set_request_timeout: Duration::from_secs(args.request_timeout),
            set_connection_keep_alive: Duration::from_secs(args.connection_keep_alive),
            info_interval: Some(Duration::from_secs(args.info_interval)),
            identify_interval: Some(Duration::from_secs(args.identify_interval)),
            metrics: false,
        })
    }
}
