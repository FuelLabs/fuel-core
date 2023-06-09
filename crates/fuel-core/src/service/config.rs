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

pub use fuel_core_poa::Trigger;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub max_database_cache_size: usize,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    pub manual_blocks_enabled: bool,
    pub block_production: Trigger,
    pub vm: VMConfig,
    pub txpool: fuel_core_txpool::Config,
    pub block_producer: fuel_core_producer::Config,
    pub block_executor: fuel_core_executor::Config,
    pub block_importer: fuel_core_importer::Config,
    #[cfg(feature = "relayer")]
    pub relayer: fuel_core_relayer::Config,
    #[cfg(feature = "p2p")]
    pub p2p: Option<P2PConfig<NotInitialized>>,
    #[cfg(feature = "p2p")]
    pub sync: fuel_core_sync::Config,
    pub consensus_key: Option<Secret<SecretKeyWrapper>>,
    pub name: String,
    pub verifier: fuel_core_consensus_module::RelayerVerifierConfig,
}

impl Config {
    pub fn local_node() -> Self {
        let chain_conf = ChainConfig::local_testnet();
        let utxo_validation = false;
        let min_gas_price = 0;
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            // Set the cache for tests = 10MB
            max_database_cache_size: 10 * 1024 * 1024,
            database_path: Default::default(),
            #[cfg(feature = "rocksdb")]
            database_type: DbType::RocksDb,
            #[cfg(not(feature = "rocksdb"))]
            database_type: DbType::InMemory,
            chain_conf: chain_conf.clone(),
            manual_blocks_enabled: false,
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
            block_executor: Default::default(),
            block_importer: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: Default::default(),
            #[cfg(feature = "p2p")]
            p2p: Some(P2PConfig::<NotInitialized>::default("test_network")),
            #[cfg(feature = "p2p")]
            sync: fuel_core_sync::Config::default(),
            consensus_key: Some(Secret::new(default_consensus_dev_key().into())),
            name: String::default(),
            verifier: Default::default(),
        }
    }
}

impl TryFrom<&Config> for fuel_core_poa::Config {
    type Error = anyhow::Error;

    fn try_from(config: &Config) -> Result<Self, Self::Error> {
        // If manual block production then require trigger never or instant.
        anyhow::ensure!(
            !config.manual_blocks_enabled
                || matches!(
                    config.block_production,
                    Trigger::Never | Trigger::Instant | Trigger::Interval { .. }
                ),
            "Cannot use manual block production unless trigger mode is never, instant or interval."
        );

        Ok(fuel_core_poa::Config {
            trigger: config.block_production,
            block_gas_limit: config.chain_conf.block_gas_limit,
            signing_key: config.consensus_key.clone(),
            metrics: false,
            consensus_params: config.chain_conf.transaction_parameters,
            ..Default::default()
        })
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
