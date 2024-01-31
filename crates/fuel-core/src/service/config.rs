use clap::ValueEnum;
use fuel_core_chain_config::{
    default_consensus_dev_key,
    ChainConfig,
};
use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    secrecy::Secret,
};
use std::{
    net::{
        Ipv4Addr,
        SocketAddr,
    },
    path::PathBuf,
    time::Duration,
};
use strum_macros::{
    Display,
    EnumString,
    EnumVariantNames,
};

#[cfg(feature = "p2p")]
use fuel_core_p2p::config::{
    Config as P2PConfig,
    NotInitialized,
};

#[cfg(feature = "relayer")]
use fuel_core_relayer::Config as RelayerConfig;

pub use fuel_core_consensus_module::RelayerConsensusConfig;
pub use fuel_core_importer;
pub use fuel_core_poa::Trigger;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub api_request_timeout: Duration,
    pub max_database_cache_size: usize,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    /// When `true`:
    /// - Enables manual block production.
    /// - Enables debugger endpoint.
    /// - Allows setting `utxo_validation` to `false`.
    pub debug: bool,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    pub block_production: Trigger,
    pub vm: VMConfig,
    pub txpool: fuel_core_txpool::Config,
    pub block_producer: fuel_core_producer::Config,
    pub block_importer: fuel_core_importer::Config,
    #[cfg(feature = "relayer")]
    pub relayer: Option<RelayerConfig>,
    #[cfg(feature = "p2p")]
    pub p2p: Option<P2PConfig<NotInitialized>>,
    #[cfg(feature = "p2p")]
    pub sync: fuel_core_sync::Config,
    pub consensus_key: Option<Secret<SecretKeyWrapper>>,
    pub name: String,
    pub relayer_consensus_config: fuel_core_consensus_module::RelayerConsensusConfig,
    /// The number of reserved peers to connect to before starting to sync.
    pub min_connected_reserved_peers: usize,
    /// Time to wait after receiving the latest block before considered to be Synced.
    pub time_until_synced: Duration,
    /// Time to wait after submitting a query before debug info will be logged about query.
    pub query_log_threshold_time: Duration,
}

impl Config {
    pub fn local_node() -> Self {
        let chain_conf = ChainConfig::local_testnet();
        let block_importer = fuel_core_importer::Config::new(&chain_conf);
        let utxo_validation = false;
        let min_gas_price = 0;

        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            api_request_timeout: Duration::from_secs(60),
            // Set the cache for tests = 10MB
            max_database_cache_size: 10 * 1024 * 1024,
            database_path: Default::default(),
            #[cfg(feature = "rocksdb")]
            database_type: DbType::RocksDb,
            #[cfg(not(feature = "rocksdb"))]
            database_type: DbType::InMemory,
            debug: true,
            chain_conf: chain_conf.clone(),
            block_production: Trigger::Instant,
            vm: Default::default(),
            utxo_validation,
            txpool: fuel_core_txpool::Config {
                chain_config: chain_conf,
                min_gas_price,
                utxo_validation,
                transaction_ttl: Duration::from_secs(60 * 100000000),
                ..fuel_core_txpool::Config::default()
            },
            block_producer: Default::default(),
            block_importer,
            #[cfg(feature = "relayer")]
            relayer: None,
            #[cfg(feature = "p2p")]
            p2p: Some(P2PConfig::<NotInitialized>::default("test_network")),
            #[cfg(feature = "p2p")]
            sync: fuel_core_sync::Config::default(),
            consensus_key: Some(Secret::new(default_consensus_dev_key().into())),
            name: String::default(),
            relayer_consensus_config: Default::default(),
            min_connected_reserved_peers: 0,
            time_until_synced: Duration::ZERO,
            query_log_threshold_time: Duration::from_secs(2),
        }
    }

    // TODO: Rework our configs system to avoid nesting of the same configs.
    pub fn make_config_consistent(mut self) -> Config {
        if !self.debug && !self.utxo_validation {
            tracing::warn!(
                "The `utxo_validation` should be `true` with disabled `debug`"
            );
            self.utxo_validation = true;
        }

        if self.txpool.chain_config != self.chain_conf {
            tracing::warn!("The `ChainConfig` of `TxPool` was inconsistent");
            self.txpool.chain_config = self.chain_conf.clone();
        }
        if self.txpool.utxo_validation != self.utxo_validation {
            tracing::warn!("The `utxo_validation` of `TxPool` was inconsistent");
            self.txpool.utxo_validation = self.utxo_validation;
        }
        if self.block_producer.utxo_validation != self.utxo_validation {
            tracing::warn!("The `utxo_validation` of `BlockProducer` was inconsistent");
            self.block_producer.utxo_validation = self.utxo_validation;
        }

        self
    }
}

impl From<&Config> for fuel_core_poa::Config {
    fn from(config: &Config) -> Self {
        fuel_core_poa::Config {
            trigger: config.block_production,
            block_gas_limit: config.chain_conf.block_gas_limit,
            signing_key: config.consensus_key.clone(),
            metrics: false,
            consensus_params: config.chain_conf.consensus_parameters.clone(),
            min_connected_reserved_peers: config.min_connected_reserved_peers,
            time_until_synced: config.time_until_synced,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VMConfig {
    pub backtrace: bool,
}

#[derive(
    Clone, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}
