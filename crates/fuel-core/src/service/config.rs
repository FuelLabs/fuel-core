use clap::ValueEnum;
use fuel_core_chain_config::ChainConfig;
use fuel_core_types::{
    blockchain::primitives::SecretKeyWrapper,
    fuel_vm::SecretKey,
    secrecy::Secret,
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
use fuel_core_p2p::config::{
    Config as P2PConfig,
    NotInitialized,
};

pub use fuel_core_poa::Trigger;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
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
            block_production: Trigger::Instant,
            vm: Default::default(),
            utxo_validation,
            txpool: fuel_core_txpool::Config::new(
                chain_conf,
                min_gas_price,
                utxo_validation,
            ),
            block_producer: Default::default(),
            block_executor: Default::default(),
            block_importer: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: Default::default(),
            #[cfg(feature = "p2p")]
            p2p: P2PConfig::<NotInitialized>::default("test_network"),
            consensus_key: Some(Secret::new(default_consensus_dev_key().into())),
        }
    }
}

impl From<&Config> for fuel_core_poa::Config {
    fn from(config: &Config) -> Self {
        fuel_core_poa::Config {
            trigger: config.block_production,
            block_gas_limit: config.chain_conf.block_gas_limit,
            signing_key: config.consensus_key.clone(),
            metrics: false,
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

/// A default secret key to use for testing purposes only
pub fn default_consensus_dev_key() -> SecretKey {
    const DEV_KEY_PHRASE: &str =
        "winner alley monkey elephant sun off boil hope toward boss bronze dish";
    SecretKey::new_from_mnemonic_phrase_with_path(DEV_KEY_PHRASE, "m/44'/60'/0'/0/0")
        .expect("valid key")
}
