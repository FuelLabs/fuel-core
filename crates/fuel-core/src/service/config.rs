use fuel_core_chain_config::{
    BlockProduction,
    ChainConfig,
    PoABlockProduction,
};
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
use fuel_core_poa::Trigger;

#[derive(Clone, Debug)]
pub enum NodeRole {
    Producer,
    Validator,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub node_role: NodeRole,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    pub manual_blocks_enabled: bool,
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
            node_role: NodeRole::Producer,
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: chain_conf.clone(),
            manual_blocks_enabled: false,
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

    pub fn poa_config(&self) -> anyhow::Result<fuel_core_poa::Config> {
        let BlockProduction::ProofOfAuthority { trigger } =
            self.chain_conf.block_production.clone();

        let trigger = match self.node_role {
            NodeRole::Producer => match trigger {
                PoABlockProduction::Instant => Trigger::Instant,
                PoABlockProduction::Interval {
                    block_time: average_block_time,
                    ..
                } => Trigger::Interval {
                    block_time: average_block_time,
                },
                PoABlockProduction::Hybrid {
                    min_block_time,
                    max_block_time,
                    max_tx_idle_time,
                } => Trigger::Hybrid {
                    min_block_time,
                    max_block_time,
                    max_tx_idle_time,
                },
            },
            NodeRole::Validator => Trigger::Never,
        };

        // If manual block production then require trigger never or instant.
        anyhow::ensure!(
            !self.manual_blocks_enabled
                || !matches!(trigger, Trigger::Never | Trigger::Instant),
            "Cannot use manual block production unless trigger mode is never or instant."
        );

        Ok(fuel_core_poa::Config {
            trigger,
            block_gas_limit: self.chain_conf.block_gas_limit,
            signing_key: self.consensus_key.clone(),
            metrics: false,
        })
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

/// A default secret key to use for testing purposes only
pub fn default_consensus_dev_key() -> SecretKey {
    const DEV_KEY_PHRASE: &str =
        "winner alley monkey elephant sun off boil hope toward boss bronze dish";
    SecretKey::new_from_mnemonic_phrase_with_path(DEV_KEY_PHRASE, "m/44'/60'/0'/0/0")
        .expect("valid key")
}
