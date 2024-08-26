use anyhow::anyhow;
use clap::{
    builder::ArgPredicate::IsPresent,
    Args,
};
use fuel_core::{
    p2p::{
        config::{
            convert_to_libp2p_keypair,
            Config,
            NotInitialized,
            MAX_RESPONSE_SIZE,
        },
        gossipsub_config::default_gossipsub_builder,
        heartbeat,
        Multiaddr,
    },
    types::{
        fuel_crypto,
        fuel_crypto::SecretKey,
    },
};
use std::{
    net::{
        IpAddr,
        Ipv4Addr,
    },
    num::NonZeroU32,
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

const MAX_RESPONSE_SIZE_STR: &str = const_format::formatcp!("{MAX_RESPONSE_SIZE}");

#[derive(Debug, Clone, Args)]
pub struct P2PArgs {
    /// Enable P2P. By default, P2P is disabled, even when the binary is compiled with the "p2p"
    /// feature flag. Providing `--enable-p2p` will enable the P2P service.
    #[clap(long = "enable-p2p", action)]
    pub enable_p2p: bool,

    /// Peering secret key. Supports either a hex encoded secret key inline or a path to bip32 mnemonic encoded secret file.
    #[clap(long = "keypair", env, value_parser = KeypairArg::try_from_string)]
    #[arg(required_if_eq("enable_p2p", "true"))]
    #[arg(requires_if(IsPresent, "enable_p2p"))]
    pub keypair: Option<KeypairArg>,

    /// p2p network's IP Address
    #[clap(long = "address", env)]
    pub address: Option<IpAddr>,

    /// Optional address of your local node made reachable for other nodes in the network.
    #[clap(long = "public-address", env)]
    pub public_address: Option<Multiaddr>,

    /// p2p network's TCP Port
    #[clap(long = "peering-port", default_value = "30333", env)]
    pub peering_port: u16,

    /// Max Block size
    #[clap(long = "max-block-size", default_value = MAX_RESPONSE_SIZE_STR, env)]
    pub max_block_size: usize,

    /// Max number of blocks/headers in a single headers request response
    #[clap(long = "max-headers-per-request", default_value = "100", env)]
    pub max_headers_per_request: usize,

    /// Addresses of the bootstrap nodes
    /// They should contain PeerId within their `Multiaddr`
    #[clap(long = "bootstrap-nodes", value_delimiter = ',', env)]
    pub bootstrap_nodes: Vec<Multiaddr>,

    /// Addresses of the reserved nodes
    /// They should contain PeerId within their `Multiaddr`
    #[clap(long = "reserved-nodes", value_delimiter = ',', env)]
    pub reserved_nodes: Vec<Multiaddr>,

    /// With this set to `true` you create a guarded node that is only ever connected to trusted, reserved nodes.
    #[clap(long = "reserved-nodes-only-mode", env)]
    pub reserved_nodes_only_mode: bool,

    /// Allow nodes to be discoverable on the local network
    #[clap(long = "enable-mdns", env)]
    pub enable_mdns: bool,

    /// Max number of unique peers connected
    /// This number should be at least number of `mesh_n` from `Gossipsub` configuration.
    /// The total number of connections will be `(max_peers_connected + reserved_nodes.len()) * max_connections_per_peer`
    #[clap(long = "max-peers-connected", default_value = "50", env)]
    pub max_peers_connected: u32,

    /// Max number of connections per single peer
    /// The total number of connections will be `(max_peers_connected + reserved_nodes.len()) * max_connections_per_peer`
    #[clap(long = "max-connections-per-peer", default_value = "3", env)]
    pub max_connections_per_peer: u32,

    /// Set the delay between random walks for p2p node discovery in seconds.
    /// If it's not set the random walk will be disabled.
    /// Also if `reserved_nodes_only_mode` is set to `true`,
    /// the random walk will be disabled.
    #[clap(long = "random-walk", default_value = "0", env)]
    pub random_walk: u64,

    /// Choose to include private IPv4/IPv6 addresses as discoverable
    /// except for the ones stored in `bootstrap_nodes`
    #[clap(long = "allow-private-addresses", env)]
    pub allow_private_addresses: bool,

    /// Choose how long will connection keep alive if idle
    #[clap(long = "connection-idle-timeout", default_value = "120", env)]
    pub connection_idle_timeout: u64,

    /// Choose how often to receive PeerInfo from other nodes
    #[clap(long = "info-interval", default_value = "3", env)]
    pub info_interval: u64,

    /// Choose the interval at which identification requests are sent to
    /// the remote on established connections after the first request
    #[clap(long = "identify-interval", default_value = "5", env)]
    pub identify_interval: u64,

    /// Choose max mesh size for gossipsub protocol
    #[clap(long = "max-mesh-size", default_value = "12", env)]
    pub max_mesh_size: usize,

    /// Choose min mesh size for gossipsub protocol
    #[clap(long = "min-mesh-size", default_value = "4", env)]
    pub min_mesh_size: usize,

    /// Choose ideal mesh size for gossipsub protocol
    #[clap(long = "ideal-mesh-size", default_value = "6", env)]
    pub ideal_mesh_size: usize,

    /// Number of heartbeats to keep in the gossipsub `memcache`
    #[clap(long = "history-length", default_value = "5", env)]
    pub history_length: usize,

    /// Number of past heartbeats to gossip about
    #[clap(long = "history-gossip", default_value = "3", env)]
    pub history_gossip: usize,

    /// Time between each gossipsub heartbeat, in milliseconds
    #[clap(long = "gossip-heartbeat-interval", default_value = "500ms", env)]
    pub gossip_heartbeat_interval: humantime::Duration,

    /// The maximum byte size for each gossip (default is 18 MiB)
    #[clap(long = "max-transmit-size", default_value = MAX_RESPONSE_SIZE_STR, env)]
    pub max_transmit_size: usize,

    /// Choose timeout for sent requests in RequestResponse protocol
    #[clap(long = "request-timeout", default_value = "20", env)]
    pub request_timeout: u64,

    /// Choose max concurrent streams for RequestResponse protocol
    #[clap(long = "request-max-concurrent-streams", default_value = "256", env)]
    pub max_concurrent_streams: usize,

    /// Choose how long RequestResponse protocol connections will live if idle
    #[clap(long = "connection-keep-alive", default_value = "20", env)]
    pub connection_keep_alive: u64,

    /// Sending of `BlockHeight` should not take longer than this duration, in seconds.
    #[clap(long = "heartbeat-send-duration", default_value = "2", env)]
    pub heartbeat_send_duration: u64,

    /// Idle time in seconds before sending next `BlockHeight`
    #[clap(long = "heartbeat-idle-duration", default_value = "1", env)]
    pub heartbeat_idle_duration: u64,

    /// Max failures allowed at `Heartbeat` protocol.
    /// If reached, the protocol will request disconnect.
    /// Cannot be zero.
    #[clap(long = "heartbeat-max-failures", default_value = "5", env)]
    pub heartbeat_max_failures: NonZeroU32,

    /// For peer reputations, the interval at which to check heartbeat health for all peers
    #[clap(long = "heartbeat-check-interval", default_value = "5", env)]
    pub heartbeat_check_interval: u64,

    /// For peer reputations, the maximum average interval between heartbeats for a peer before penalty
    #[clap(long = "heartbeat-max-avg-interval", default_value = "20", env)]
    pub heartbeat_max_avg_interval: u64,

    /// For peer reputations, the maximum time since last heartbeat before penalty
    #[clap(long = "heartbeat-max-time-since-last", default_value = "40", env)]
    pub heartbeat_max_time_since_last: u64,
}

#[derive(Debug, Clone, Args)]
pub struct SyncArgs {
    /// The maximum number of get transaction requests to make in a single batch.
    #[clap(long = "sync-block-stream-buffer-size", default_value = "10", env)]
    pub block_stream_buffer_size: usize,
    /// The maximum number of headers to request in a single batch.
    #[clap(long = "sync-header-batch-size", default_value = "10", env)]
    pub header_batch_size: u32,
}

#[derive(Clone, Debug)]
pub enum KeypairArg {
    Path(PathBuf),
    InlineSecret(SecretKey),
}

impl KeypairArg {
    pub fn try_from_string(s: &str) -> anyhow::Result<KeypairArg> {
        // first try to parse as inline secret
        // then try to parse as a pathbuf

        let secret = SecretKey::from_str(s);
        if let Ok(secret) = secret {
            return Ok(KeypairArg::InlineSecret(secret))
        }
        let path = PathBuf::from_str(s);
        if let Ok(pathbuf) = path {
            if pathbuf.exists() {
                return Ok(KeypairArg::Path(pathbuf))
            } else {
                return Err(anyhow!(
                    "path `{pathbuf:?}` does not exist for keypair argument"
                ))
            }
        }
        Err(anyhow!(
            "invalid keypair argument, neither a valid key or path"
        ))
    }
}

impl From<SyncArgs> for fuel_core::sync::Config {
    fn from(value: SyncArgs) -> Self {
        Self {
            block_stream_buffer_size: value.block_stream_buffer_size,
            header_batch_size: value.header_batch_size as usize,
        }
    }
}

impl P2PArgs {
    pub fn into_config(
        self,
        network_name: String,
        metrics: bool,
    ) -> anyhow::Result<Option<Config<NotInitialized>>> {
        if !self.enable_p2p {
            tracing::info!("P2P service disabled");
            return Ok(None)
        }

        let local_keypair = {
            match self.keypair.expect("mandatory value") {
                KeypairArg::Path(path) => {
                    let phrase = std::fs::read_to_string(path)?;
                    let secret_key =
                        fuel_crypto::SecretKey::new_from_mnemonic_phrase_with_path(
                            &phrase,
                            "m/44'/60'/0'/0/0",
                        )?;

                    convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
                KeypairArg::InlineSecret(secret_key) => {
                    convert_to_libp2p_keypair(&mut secret_key.to_vec())?
                }
            }
        };

        let gossipsub_config = default_gossipsub_builder()
            .mesh_n(self.ideal_mesh_size)
            .mesh_n_low(self.min_mesh_size)
            .mesh_n_high(self.max_mesh_size)
            .history_length(self.history_length)
            .history_gossip(self.history_gossip)
            .heartbeat_interval(self.gossip_heartbeat_interval.into())
            .max_transmit_size(self.max_transmit_size)
            .build()
            .expect("valid gossipsub configuration");

        let random_walk = if self.random_walk == 0 {
            None
        } else {
            Some(Duration::from_secs(self.random_walk))
        };

        let heartbeat_config = {
            let send_duration = Duration::from_secs(self.heartbeat_send_duration);
            let idle_duration = Duration::from_secs(self.heartbeat_idle_duration);
            heartbeat::Config::new(
                send_duration,
                idle_duration,
                self.heartbeat_max_failures,
            )
        };

        let config = Config {
            keypair: local_keypair,
            network_name,
            checksum: Default::default(),
            address: self
                .address
                .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::from([0, 0, 0, 0]))),
            public_address: self.public_address,
            tcp_port: self.peering_port,
            max_block_size: self.max_block_size,
            max_headers_per_request: self.max_headers_per_request,
            bootstrap_nodes: self.bootstrap_nodes,
            reserved_nodes: self.reserved_nodes,
            reserved_nodes_only_mode: self.reserved_nodes_only_mode,
            enable_mdns: self.enable_mdns,
            max_peers_connected: self.max_peers_connected,
            max_connections_per_peer: self.max_connections_per_peer,
            allow_private_addresses: self.allow_private_addresses,
            random_walk,
            connection_idle_timeout: Some(Duration::from_secs(
                self.connection_idle_timeout,
            )),
            gossipsub_config,
            heartbeat_config,
            set_request_timeout: Duration::from_secs(self.request_timeout),
            max_concurrent_streams: self.max_concurrent_streams,
            set_connection_keep_alive: Duration::from_secs(self.connection_keep_alive),
            heartbeat_check_interval: Duration::from_secs(self.heartbeat_check_interval),
            heartbeat_max_avg_interval: Duration::from_secs(
                self.heartbeat_max_avg_interval,
            ),
            heartbeat_max_time_since_last: Duration::from_secs(
                self.heartbeat_max_time_since_last,
            ),
            info_interval: Some(Duration::from_secs(self.info_interval)),
            identify_interval: Some(Duration::from_secs(self.identify_interval)),
            metrics,
            state: NotInitialized,
        };
        Ok(Some(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_path() {
        // Given
        let invalid_path = "/invalid/path/to/keypair";
        // When
        let keypair = KeypairArg::try_from_string(invalid_path);

        // Then
        let err = keypair.expect_err("The path is incorrect it should fail");
        assert!(err
            .to_string()
            .contains("does not exist for keypair argument"));
    }
}
