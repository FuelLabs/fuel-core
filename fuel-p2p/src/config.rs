use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed},
    identity::Keypair,
    mplex, noise, yamux, Multiaddr, PeerId, Transport,
};
use std::{net::IpAddr, time::Duration};

pub const REQ_RES_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum number of frames buffered per substream.
const MAX_NUM_OF_FRAMES_BUFFERED: usize = 256;

/// Adds a timeout to the setup and protocol upgrade process for all
/// inbound and outbound connections established through the transport.
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(20);

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

    // `Gossipsub` related fields
    pub topics: Vec<String>,
    pub ideal_mesh_size: usize,
    pub min_mesh_size: usize,
    pub max_mesh_size: usize,

    // RequestResponse related fields
    /// Sets the timeout for inbound and outbound requests.
    pub set_request_timeout: Option<Duration>,
    /// Sets the keep-alive timeout of idle connections.
    pub set_connection_keep_alive: Option<Duration>,
}

/// Transport for libp2p communication:
/// TCP/IP, Websocket
/// Noise as encryption layer
/// mplex or yamux for multiplexing
pub async fn build_transport(local_keypair: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let transport = {
        let tcp = libp2p::tcp::TcpConfig::new().nodelay(true);
        let ws_tcp = libp2p::websocket::WsConfig::new(tcp.clone()).or_transport(tcp);
        libp2p::dns::DnsConfig::system(ws_tcp).await.unwrap()
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
