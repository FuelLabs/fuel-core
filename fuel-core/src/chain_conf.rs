use fuel_types::{Address, Bytes32, Color};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub chain_name: String,
    pub block_production: ProductionStrategy,
    pub parent_network: BaseChainConfig,
    pub initial_state: StateConfig,
}

impl ChainConfig {
    pub fn local_testnet() -> Self {
        Self {
            chain_name: "local_testnet".to_string(),
            block_production: ProductionStrategy::Instant,
            parent_network: BaseChainConfig::Test,
            initial_state: StateConfig {
                coins: vec![],
                contracts: vec![],
            },
        }
    }
}

impl FromStr for ChainConfig {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_testnet" => Ok(Self::local_testnet()),
            s => {
                // Attempt to load chain config from path
                let path = PathBuf::from(s.to_string());
                let contents = std::fs::read(path)?;
                serde_json::from_slice(&contents)
                    .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ProductionStrategy {
    Instant,
    Manual,
    RoundRobin,
    ProofOfStake,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BaseChainConfig {
    Test,
    Ethereum {
        /// Contract address for publishing blocks performing IVG
        address: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateConfig {
    pub coins: Vec<CoinConfig>,
    pub contracts: Vec<ContractConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoinConfig {
    pub owner: Address,
    pub amount: u64,
    pub color: Color,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractConfig {
    code: Vec<u8>,
    state: Option<ContractStateConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractStateConfig {
    state: HashMap<Bytes32, Bytes32>,
}
