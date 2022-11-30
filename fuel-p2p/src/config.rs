use crate::gossipsub::{
    config::default_gossipsub_config,
    topics::{
        CON_VOTE_GOSSIP_TOPIC,
        NEW_BLOCK_GOSSIP_TOPIC,
        NEW_TX_GOSSIP_TOPIC,
    },
};

use libp2p::{
    core::{
        muxing::StreamMuxerBox,
        transport::Boxed,
    },
    identity::{
        secp256k1::SecretKey,
        Keypair,
    },
    mplex,
    noise,
    tcp::{
        tokio::Transport as TokioTcpTransport,
        Config as TcpConfig,
    },
    yamux,
    Multiaddr,
    PeerId,
    Transport,
};

use std::{
    net::{
        IpAddr,
        Ipv4Addr,
    },
    time::Duration,
};

use libp2p::gossipsub::GossipsubConfig;

const REQ_RES_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum number of frames buffered per substream.
const MAX_NUM_OF_FRAMES_BUFFERED: usize = 256;

/// Adds a timeout to the setup and protocol upgrade process for all
/// inbound and outbound connections established through the transport.
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct P2PConfig {
    pub local_keypair: Keypair,

    /// Name of the Network
    pub network_name: String,

    /// IP address for Swarm to listen on
    pub address: IpAddr,

    /// Optional address of your local node made reachable for other nodes in the network.
    pub public_address: Option<Multiaddr>,

    /// The TCP port that Swarm listens on
    pub tcp_port: u16,

    /// Max Size of a FuelBlock in bytes
    pub max_block_size: usize,

    // `DiscoveryBehaviour` related fields
    pub bootstrap_nodes: Vec<Multiaddr>,
    pub enable_mdns: bool,
    pub max_peers_connected: usize,
    pub allow_private_addresses: bool,
    pub enable_random_walk: bool,
    pub connection_idle_timeout: Option<Duration>,

    // `PeerInfo` fields
    /// The interval at which identification requests are sent to
    /// the remote on established connections after the first request
    pub identify_interval: Option<Duration>,
    /// The duration between the last successful outbound or inbound ping
    /// and the next outbound ping
    pub info_interval: Option<Duration>,

    // `Gossipsub` config and topics
    pub gossipsub_config: GossipsubConfig,
    pub topics: Vec<String>,

    // RequestResponse related fields
    /// Sets the timeout for inbound and outbound requests.
    pub set_request_timeout: Duration,
    /// Sets the keep-alive timeout of idle connections.
    pub set_connection_keep_alive: Duration,

    /// Enables prometheus metrics for this fuel-service
    pub metrics: bool,
}

/// Takes secret key bytes generated outside of libp2p.
/// And converts it into libp2p's `Keypair::Secp256k1`.
pub fn convert_to_libp2p_keypair(
    secret_key_bytes: impl AsMut<[u8]>,
) -> anyhow::Result<Keypair> {
    let secret_key = SecretKey::from_bytes(secret_key_bytes)?;

    Ok(Keypair::Secp256k1(secret_key.into()))
}

impl P2PConfig {
    pub fn default_with_network(network_name: &str) -> Self {
        let local_keypair = Keypair::generate_secp256k1();

        P2PConfig {
            local_keypair,
            network_name: network_name.into(),
            address: IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0])),
            public_address: None,
            tcp_port: 0,
            max_block_size: 100_000,
            bootstrap_nodes: vec![],
            enable_mdns: false,
            max_peers_connected: 50,
            allow_private_addresses: true,
            enable_random_walk: true,
            connection_idle_timeout: Some(Duration::from_secs(120)),
            topics: vec![
                NEW_TX_GOSSIP_TOPIC.into(),
                NEW_BLOCK_GOSSIP_TOPIC.into(),
                CON_VOTE_GOSSIP_TOPIC.into(),
            ],
            gossipsub_config: default_gossipsub_config(),
            set_request_timeout: REQ_RES_TIMEOUT,
            set_connection_keep_alive: REQ_RES_TIMEOUT,
            info_interval: Some(Duration::from_secs(3)),
            identify_interval: Some(Duration::from_secs(5)),
            metrics: false,
        }
    }
}

/// Transport for libp2p communication:
/// TCP/IP, Websocket
/// Noise as encryption layer
/// mplex or yamux for multiplexing
pub(crate) fn build_transport(local_keypair: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let transport = {
        let generate_tcp_transport =
            || TokioTcpTransport::new(TcpConfig::new().port_reuse(true).nodelay(true));

        let tcp = generate_tcp_transport();

        let ws_tcp =
            libp2p::websocket::WsConfig::new(generate_tcp_transport()).or_transport(tcp);

        libp2p::dns::TokioDnsConfig::system(ws_tcp).unwrap()
    };

    let auth_config = {
        let dh_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&local_keypair)
            .expect("Noise key generation failed");

        noise::NoiseConfig::xx(dh_keys).into_authenticated()
    };

    let multiplex_config = {
        let mut mplex_config = mplex::MplexConfig::new();
        mplex_config.set_max_buffer_size(MAX_NUM_OF_FRAMES_BUFFERED);

        let yamux_config = yamux::YamuxConfig::default();
        libp2p::core::upgrade::SelectUpgrade::new(yamux_config, mplex_config)
    };

    transport
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(multiplex_config)
        .timeout(TRANSPORT_TIMEOUT)
        .boxed()
}
