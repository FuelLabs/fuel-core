use crate::model::fuel_block::BlockHeight;
use fuel_types::{Address, Bytes32, Color, Salt};
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
                height: Default::default(),
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

// TODO: do streaming deserialization to handle large state configs
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Vec<CoinConfig>,
    /// Contract state
    pub contracts: Vec<ContractConfig>,
    /// Starting block height (useful for forked networks)
    pub height: Option<BlockHeight>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoinConfig {
    /// auto-generated if None
    pub utxo_id: Option<Bytes32>,
    /// used if coin is forked from another chain to preserve id
    pub block_created: Option<BlockHeight>,
    pub maturity: Option<BlockHeight>,
    pub owner: Address,
    pub amount: u64,
    pub color: Color,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractConfig {
    pub code: Vec<u8>,
    pub salt: Salt,
    pub state: Option<HashMap<Bytes32, Bytes32>>,
}
//
// #[derive(Clone, Debug, Deserialize, Serialize)]
// pub struct ContractStateConfig {
//     pub state: ,
// }
