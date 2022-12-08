use crate::gossipsub::{
    config::default_gossipsub_config,
    topics::{
        CON_VOTE_GOSSIP_TOPIC,
        NEW_BLOCK_GOSSIP_TOPIC,
        NEW_TX_GOSSIP_TOPIC,
    },
};

use fuel_core_interfaces::common::secrecy::Zeroize;
use futures::{
    future,
    AsyncRead,
    AsyncWrite,
    Future,
    FutureExt,
    TryFutureExt,
};
use libp2p::{
    core::{
        muxing::StreamMuxerBox,
        transport::Boxed,
        upgrade::{
            read_length_prefixed,
            write_length_prefixed,
        },
        UpgradeInfo,
    },
    identity::{
        secp256k1::SecretKey,
        Keypair,
    },
    mplex,
    noise::{
        self,
        NoiseAuthenticated,
        NoiseError,
        NoiseOutput,
        Protocol,
    },
    tcp::{
        tokio::Transport as TokioTcpTransport,
        Config as TcpConfig,
    },
    yamux,
    InboundUpgrade,
    Multiaddr,
    OutboundUpgrade,
    PeerId,
    Transport,
};

use std::{
    collections::HashSet,
    error::Error,
    fmt,
    io,
    net::{
        IpAddr,
        Ipv4Addr,
    },
    pin::Pin,
    time::Duration,
};

use libp2p::gossipsub::GossipsubConfig;

const REQ_RES_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum number of frames buffered per substream.
const MAX_NUM_OF_FRAMES_BUFFERED: usize = 256;

/// Adds a timeout to the setup and protocol upgrade process for all
/// inbound and outbound connections established through the transport.
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(20);

/// Sha256 hash of chain id and chain config
type Checksum = [u8; 32];

#[derive(Clone, Debug)]
pub struct P2PConfig {
    pub local_keypair: Keypair,

    /// Name of the Network
    pub network_name: String,

    /// Checksum (sha256) of Chain ID + Chain Config
    pub checksum: Checksum,

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
    pub random_walk: Option<Duration>,
    pub connection_idle_timeout: Option<Duration>,

    // 'Reserved Nodes' mode
    /// Priority nodes that the node should maintain connection to
    pub reserved_nodes: Vec<Multiaddr>,
    /// Should the node only accept connection requests from the Reserved Nodes
    pub reserved_nodes_only_mode: bool,

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
            checksum: [0u8; 32],
            address: IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0])),
            public_address: None,
            tcp_port: 0,
            max_block_size: 100_000,
            bootstrap_nodes: vec![],
            enable_mdns: false,
            max_peers_connected: 50,
            allow_private_addresses: true,
            random_walk: Some(Duration::from_secs(5)),
            connection_idle_timeout: Some(Duration::from_secs(120)),
            reserved_nodes: vec![],
            reserved_nodes_only_mode: false,
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
pub(crate) fn build_transport(p2p_config: &P2PConfig) -> Boxed<(PeerId, StreamMuxerBox)> {
    let transport = {
        let generate_tcp_transport =
            || TokioTcpTransport::new(TcpConfig::new().port_reuse(true).nodelay(true));

        let tcp = generate_tcp_transport();

        let ws_tcp =
            libp2p::websocket::WsConfig::new(generate_tcp_transport()).or_transport(tcp);

        libp2p::dns::TokioDnsConfig::system(ws_tcp).unwrap()
    }
    .upgrade(libp2p::core::upgrade::Version::V1);

    let noise_authenticated = {
        let dh_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&p2p_config.local_keypair)
            .expect("Noise key generation failed");

        noise::NoiseConfig::xx(dh_keys).into_authenticated()
    };

    let multiplex_config = {
        let mut mplex_config = mplex::MplexConfig::new();
        mplex_config.set_max_buffer_size(MAX_NUM_OF_FRAMES_BUFFERED);

        let yamux_config = yamux::YamuxConfig::default();
        libp2p::core::upgrade::SelectUpgrade::new(yamux_config, mplex_config)
    };

    let fuel_upgrade = FuelUpgrade::new(p2p_config.checksum);

    if p2p_config.reserved_nodes_only_mode {
        transport
            .authenticate(NoiseWithReservedNodes::new(
                noise_authenticated,
                &p2p_config.reserved_nodes,
            ))
            .apply(fuel_upgrade)
            .multiplex(multiplex_config)
            .timeout(TRANSPORT_TIMEOUT)
            .boxed()
    } else {
        transport
            .authenticate(noise_authenticated)
            .apply(fuel_upgrade)
            .multiplex(multiplex_config)
            .timeout(TRANSPORT_TIMEOUT)
            .boxed()
    }
}

/// Wrapper over Noise protocol authentication.
/// Used in case where the local node wants to limit
/// who establishes the connection with it.
/// During the Identity (PeerId) exchange, the node can check if the inbound connection
/// comes from a whitelisted peer, in case it's not the connection is rejected.
/// Same check is added to any outbound connections, just in case.
#[derive(Clone)]
struct NoiseWithReservedNodes<P, C: Zeroize, R> {
    noise_authenticated: NoiseAuthenticated<P, C, R>,
    reserved_nodes: HashSet<PeerId>,
}

impl<P, C: Zeroize, R> NoiseWithReservedNodes<P, C, R> {
    fn new(
        noise_authenticated: NoiseAuthenticated<P, C, R>,
        reserved_nodes: &[Multiaddr],
    ) -> Self {
        Self {
            noise_authenticated,
            reserved_nodes: reserved_nodes
                .iter()
                // Safety: as is the case with `bootstrap_nodes` it is assumed that `reserved_nodes` [`Multiadr`]
                // come with PeerId included, in case they are not the `unwrap()` will only panic when the node is started.
                .map(|address| PeerId::try_from_multiaddr(address).unwrap())
                .collect(),
        }
    }
}

/// Checks if PeerId of the remote node is contained within the reserved nodes.
/// It rejects the connection otherwise.
fn accept_reserved_node<T>(
    reserved_nodes: &HashSet<PeerId>,
    remote_peer_id: PeerId,
    io: NoiseOutput<T>,
) -> future::Ready<Result<(PeerId, NoiseOutput<T>), NoiseError>> {
    if reserved_nodes.contains(&remote_peer_id) {
        future::ok((remote_peer_id, io))
    } else {
        future::err(NoiseError::AuthenticationFailed)
    }
}

impl<P, C: Zeroize, R> UpgradeInfo for NoiseWithReservedNodes<P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo,
{
    type Info = <NoiseAuthenticated<P, C, R> as UpgradeInfo>::Info;
    type InfoIter = <NoiseAuthenticated<P, C, R> as UpgradeInfo>::InfoIter;

    fn protocol_info(&self) -> Self::InfoIter {
        self.noise_authenticated.protocol_info()
    }
}

impl<T, P, C, R> InboundUpgrade<T> for NoiseWithReservedNodes<P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo
        + InboundUpgrade<T, Output = (PeerId, NoiseOutput<T>), Error = NoiseError>
        + 'static,
    <NoiseAuthenticated<P, C, R> as InboundUpgrade<T>>::Future: Send,
    T: AsyncRead + AsyncWrite + Send + 'static,
    C: Protocol<C> + AsRef<[u8]> + Zeroize + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: T, info: Self::Info) -> Self::Future {
        Box::pin(
            self.noise_authenticated
                .upgrade_inbound(socket, info)
                .and_then(move |(remote_peer_id, io)| {
                    accept_reserved_node(&self.reserved_nodes, remote_peer_id, io)
                }),
        )
    }
}

impl<T, P, C, R> OutboundUpgrade<T> for NoiseWithReservedNodes<P, C, R>
where
    NoiseAuthenticated<P, C, R>: UpgradeInfo
        + OutboundUpgrade<T, Output = (PeerId, NoiseOutput<T>), Error = NoiseError>
        + 'static,
    <NoiseAuthenticated<P, C, R> as OutboundUpgrade<T>>::Future: Send,
    T: AsyncRead + AsyncWrite + Send + 'static,
    C: Protocol<C> + AsRef<[u8]> + Zeroize + Send + 'static,
{
    type Output = (PeerId, NoiseOutput<T>);
    type Error = NoiseError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: T, info: Self::Info) -> Self::Future {
        Box::pin(
            self.noise_authenticated
                .upgrade_outbound(socket, info)
                .and_then(move |(remote_peer_id, io)| {
                    accept_reserved_node(&self.reserved_nodes, remote_peer_id, io)
                }),
        )
    }
}

/// When two nodes want to establish a connection they need to
/// exchange the Hash of their respective Chain Id and Chain Config.
/// The connection is only accepted if their hashes match.
/// This is used to aviod peers having same network name but different configurations connecting to each other.
#[derive(Debug, Clone)]
struct FuelUpgrade {
    checksum: Checksum,
}

impl FuelUpgrade {
    fn new(checksum: Checksum) -> Self {
        Self { checksum }
    }
}

#[derive(Debug)]
enum FuelUpgradeError {
    IncorrectChecksum,
    Io(io::Error),
}

impl fmt::Display for FuelUpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FuelUpgradeError::Io(e) => write!(f, "{}", e),            
            FuelUpgradeError::IncorrectChecksum => f.write_str("Fuel node checksum does not match, either ChainId or ChainConfig are not the same, or both."),            
        }
    }
}

impl From<io::Error> for FuelUpgradeError {
    fn from(e: io::Error) -> Self {
        FuelUpgradeError::Io(e)
    }
}

impl Error for FuelUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FuelUpgradeError::Io(e) => Some(e),
            FuelUpgradeError::IncorrectChecksum => None,
        }
    }
}

impl UpgradeInfo for FuelUpgrade {
    type Info = &'static [u8];
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(b"/fuel/upgrade/0")
    }
}

impl<C> InboundUpgrade<C> for FuelUpgrade
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = FuelUpgradeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: C, _: Self::Info) -> Self::Future {
        async move {
            // Inbound node receives the checksum and compares it to its own checksum.
            // If they do not match the connection is rejected.
            let res = read_length_prefixed(&mut socket, self.checksum.len()).await?;
            if res != self.checksum {
                return Err(FuelUpgradeError::IncorrectChecksum)
            }

            Ok(socket)
        }
        .boxed()
    }
}

impl<C> OutboundUpgrade<C> for FuelUpgrade
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = FuelUpgradeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: C, _: Self::Info) -> Self::Future {
        async move {
            // Outbound node sends their own checksum for comparison with the inbound node.
            write_length_prefixed(&mut socket, &self.checksum).await?;

            // Note: outbound node does not need to receive the checksum from the inbound node,
            // since inbound node will reject the connection if the two don't match on its side.

            Ok(socket)
        }
        .boxed()
    }
}
