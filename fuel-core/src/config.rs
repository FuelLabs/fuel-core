pub mod chain_config;
pub mod serialization;

use chain_config::ChainConfig;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
};
use strum_macros::{Display, EnumString, EnumVariantNames};
#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    // default to false until predicates have fully stabilized
    pub predicates: bool,
    pub vm: VMConfig,
    pub txpool: fuel_txpool::Config,
    pub block_importer: fuel_block_importer::Config,
    pub block_producer: fuel_block_producer::Config,
    pub block_executor: fuel_block_executor::Config,
    pub bft: fuel_core_bft::Config,
    pub sync: fuel_sync::Config,
    pub relayer: fuel_relayer::Config,
}

impl Config {
    pub fn local_node() -> Self {
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: ChainConfig::local_testnet(),
            vm: Default::default(),
            utxo_validation: false,
            predicates: false,
            txpool: Default::default(),
            block_importer: Default::default(),
            block_producer: Default::default(),
            block_executor: Default::default(),
            bft: Default::default(),
            sync: Default::default(),
            relayer: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VMConfig {
    pub backtrace: bool,
}

#[derive(Clone, Debug, Display, PartialEq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}
