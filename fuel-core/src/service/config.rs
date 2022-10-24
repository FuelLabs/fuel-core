use fuel_chain_config::ChainConfig;
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
use fuel_p2p;

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
    pub p2p: fuel_p2p::config::P2PConfig,
}

impl Config {
    pub fn local_node() -> Self {
        let chain_conf = ChainConfig::local_testnet();
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: chain_conf.clone(),
            manual_blocks_enabled: false,
            vm: Default::default(),
            utxo_validation: false,
            txpool: fuel_txpool::Config {
                utxo_validation: false,
                chain_config: chain_conf,
                ..Default::default()
            },
            block_importer: Default::default(),
            block_producer: Default::default(),
            block_executor: Default::default(),
            sync: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: Default::default(),
            #[cfg(feature = "p2p")]
            p2p: fuel_p2p::config::P2PConfig::default_with_network("test_network"),
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
