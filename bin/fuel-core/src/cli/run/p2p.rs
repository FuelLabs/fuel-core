use clap::Args;
use fuel_core::{
    p2p::{
        config::{
            convert_to_libp2p_keypair,
            Config,
            NotInitialized,
        },
        gossipsub_config::default_gossipsub_builder,
        Multiaddr,
    },
    types::fuel_crypto,
};
use std::{
    net::{
        IpAddr,
        Ipv4Addr,
    },
    path::PathBuf,
    time::Duration,
};

#[derive(Debug, Clone, Args)]
pub struct P2PArgs {
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

    /// Optional address of your local node made reachable for other nodes in the network.
    #[clap(long = "public_address")]
    pub public_address: Option<Multiaddr>,

    /// p2p network's TCP Port
    #[clap(long = "peering_port", default_value = "4001")]
    pub peering_port: u16,

    /// Max Block size
    #[clap(long = "max_block_size", default_value = "100000")]
    pub max_block_size: usize,

    /// Addresses of the bootstrap nodes
    /// They should contain PeerId within their `Multiaddr`
    #[clap(long = "bootstrap_nodes")]
    pub bootstrap_nodes: Vec<Multiaddr>,

    /// Addresses of the reserved nodes
    /// They should contain PeerId within their `Multiaddr`
    #[clap(long = "reserved_nodes")]
    pub reserved_nodes: Vec<Multiaddr>,

    /// With this set to `true` you create a guarded node that is only ever connected to trusted, reserved nodes.    
    #[clap(long = "reserved_nodes_only_mode")]
    pub reserved_nodes_only_mode: bool,

    /// Allow nodes to be discoverable on the local network
    #[clap(long = "enable_mdns")]
    pub enable_mdns: bool,

    /// Maximum amount of allowed connected peers
    #[clap(long = "max_peers_connected", default_value = "50")]
    pub max_peers_connected: usize,

    /// Set the delay between random walks for p2p node discovery in seconds.
    /// If it's not set the random walk will be disabled.
    /// Also if `reserved_nodes_only_mode` is set to `true`,
    /// the random walk will be disabled.
    #[clap(long = "random_walk", default_value = "0")]
    pub random_walk: u64,

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

impl P2PArgs {
    pub fn into_config(self, metrics: bool) -> anyhow::Result<Config<NotInitialized>> {
        let local_keypair = {
            match self.keypair {
                Some(path) => {
                    let phrase = std::fs::read_to_string(path)?;

                    let secret_key =
                        fuel_crypto::SecretKey::new_from_mnemonic_phrase_with_path(
                            &phrase,
                            "m/44'/60'/0'/0/0",
                        )?;

                    convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
                _ => {
                    let mut rand = fuel_crypto::rand::thread_rng();
                    let secret_key = fuel_crypto::SecretKey::random(&mut rand);

                    convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
            }
        };

        // Reserved nodes do not count against the configured peer input/output limits.
        let reserved_nodes_count = self.reserved_nodes.len();

        let gossipsub_config = default_gossipsub_builder()
            .mesh_n(self.ideal_mesh_size + reserved_nodes_count)
            .mesh_n_low(self.min_mesh_size + reserved_nodes_count)
            .mesh_n_high(self.max_mesh_size + reserved_nodes_count)
            .history_length(self.history_length)
            .history_gossip(self.history_gossip)
            .heartbeat_interval(Duration::from_secs(self.heartbeat_interval))
            .max_transmit_size(self.max_transmit_size)
            .build()
            .expect("valid gossipsub configuration");

        let random_walk = if self.random_walk == 0 {
            None
        } else {
            Some(Duration::from_secs(self.random_walk))
        };

        Ok(Config {
            keypair: local_keypair,
            network_name: self.network,
            checksum: Default::default(),
            address: self
                .address
                .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0]))),
            public_address: self.public_address,
            tcp_port: self.peering_port,
            max_block_size: self.max_block_size,
            bootstrap_nodes: self.bootstrap_nodes,
            reserved_nodes: self.reserved_nodes,
            reserved_nodes_only_mode: self.reserved_nodes_only_mode,
            enable_mdns: self.enable_mdns,
            max_peers_connected: self.max_peers_connected,
            allow_private_addresses: self.allow_private_addresses,
            random_walk,
            connection_idle_timeout: Some(Duration::from_secs(
                self.connection_idle_timeout,
            )),
            topics: self.topics,
            gossipsub_config,
            set_request_timeout: Duration::from_secs(self.request_timeout),
            set_connection_keep_alive: Duration::from_secs(self.connection_keep_alive),
            info_interval: Some(Duration::from_secs(self.info_interval)),
            identify_interval: Some(Duration::from_secs(self.identify_interval)),
            metrics,
            state: NotInitialized,
        })
    }
}
