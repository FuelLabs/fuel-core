use fuel_chain_config::ChainConfig;
use fuel_core_interfaces::{
    common::{
        prelude::SecretKey,
        secrecy::Secret,
    },
    model::SecretKeyWrapper,
};
use std::{
    net::{
        Ipv4Addr,
        SocketAddr,
    },
    path::PathBuf,
};
use strum_macros::{
    Display,
    EnumString,
    EnumVariantNames,
};

#[cfg(feature = "p2p")]
use fuel_core_interfaces::model::Genesis;
#[cfg(feature = "p2p")]
use fuel_p2p::config::{
    Initialized,
    P2PConfig,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    pub manual_blocks_enabled: bool,
    pub vm: VMConfig,
    pub txpool: fuel_txpool::Config,
    pub block_importer: fuel_block_importer::Config,
    pub block_producer: fuel_block_producer::Config,
    pub block_executor: fuel_block_executor::Config,
    pub sync: fuel_sync::Config,
    #[cfg(feature = "relayer")]
    pub relayer: fuel_relayer::Config,
    #[cfg(feature = "p2p")]
    pub p2p: P2PConfig<NotInitialized>,
    pub consensus_key: Option<Secret<SecretKeyWrapper>>,
}

impl Config {
    pub fn local_node() -> Self {
        let chain_conf = ChainConfig::local_testnet();
        let utxo_validation = false;
        let min_gas_price = 0;
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: chain_conf.clone(),
            manual_blocks_enabled: false,
            vm: Default::default(),
            utxo_validation,
            txpool: fuel_txpool::Config::new(chain_conf, min_gas_price, utxo_validation),
            block_importer: Default::default(),
            block_producer: Default::default(),
            block_executor: Default::default(),
            sync: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: Default::default(),
            #[cfg(feature = "p2p")]
            p2p: P2PConfig::default_with_network("test_network"),
            consensus_key: Some(Secret::new(default_consensus_dev_key().into())),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VMConfig {
    pub backtrace: bool,
}

#[derive(Clone, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}

#[cfg(feature = "p2p")]
#[derive(Debug, Clone, Default)]
pub struct NotInitialized;

#[cfg(feature = "p2p")]
pub trait InitializeP2PConfig {
    /// Inits the `P2PConfig` with some lazily loaded data.
    fn init(self, genesis: Genesis) -> anyhow::Result<P2PConfig>;
}

#[cfg(feature = "p2p")]
impl InitializeP2PConfig for P2PConfig<NotInitialized> {
    fn init(self, mut genesis: Genesis) -> anyhow::Result<P2PConfig> {
        use fuel_chain_config::GenesisCommitment;

        Ok(P2PConfig {
            keypair: self.keypair,
            network_name: self.network_name,
            address: self.address,
            public_address: self.public_address,
            tcp_port: self.tcp_port,
            max_block_size: self.max_block_size,
            bootstrap_nodes: self.bootstrap_nodes,
            enable_mdns: self.enable_mdns,
            max_peers_connected: self.max_peers_connected,
            allow_private_addresses: self.allow_private_addresses,
            random_walk: self.random_walk,
            connection_idle_timeout: self.connection_idle_timeout,
            reserved_nodes: self.reserved_nodes,
            reserved_nodes_only_mode: self.reserved_nodes_only_mode,
            identify_interval: self.identify_interval,
            info_interval: self.info_interval,
            gossipsub_config: self.gossipsub_config,
            topics: self.topics,
            set_request_timeout: self.set_request_timeout,
            set_connection_keep_alive: self.set_connection_keep_alive,
            metrics: self.metrics,
            extra: Initialized {
                checksum: genesis.root()?.into(),
            },
        })
    }
}

/// A default secret key to use for testing purposes only
pub fn default_consensus_dev_key() -> SecretKey {
    const DEV_KEY_PHRASE: &str =
        "winner alley monkey elephant sun off boil hope toward boss bronze dish";
    SecretKey::new_from_mnemonic_phrase_with_path(DEV_KEY_PHRASE, "m/44'/60'/0'/0/0")
        .expect("valid key")
}
