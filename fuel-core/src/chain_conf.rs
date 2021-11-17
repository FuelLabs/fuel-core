use self::serialization::HexType;
use crate::model::fuel_block::BlockHeight;
use fuel_types::{Address, Bytes32, Color, Salt};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use std::{io::ErrorKind, path::PathBuf, str::FromStr};

pub mod serialization;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
            parent_network: BaseChainConfig::LocalTest,
            initial_state: StateConfig {
                coins: vec![],
                contracts: vec![],
                height: None,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum ProductionStrategy {
    Instant,
    Manual,
    RoundRobin,
    ProofOfStake,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum BaseChainConfig {
    LocalTest,
    // TODO: determine which ethereum config values we'll need here
    Ethereum,
}

// TODO: do streaming deserialization to handle large state configs
#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Vec<CoinConfig>,
    /// Contract state
    pub contracts: Vec<ContractConfig>,
    /// Starting block height (useful for forked networks)
    pub height: Option<BlockHeight>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    pub utxo_id: Option<Bytes32>,
    /// used if coin is forked from another chain to preserve id
    pub block_created: Option<BlockHeight>,
    pub maturity: Option<BlockHeight>,
    #[serde_as(as = "HexType")]
    pub owner: Address,
    pub amount: u64,
    #[serde_as(as = "HexType")]
    pub color: Color,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ContractConfig {
    #[serde_as(as = "HexType")]
    pub code: Vec<u8>,
    #[serde_as(as = "HexType")]
    pub salt: Salt,
    #[serde_as(as = "Option<Vec<(HexType, HexType)>>")]
    pub state: Option<Vec<(Bytes32, Bytes32)>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_asm::Opcode;
    use fuel_vm::prelude::Contract;
    use rand::prelude::StdRng;
    use rand::{Rng, RngCore, SeedableRng};

    #[test]
    fn snapshot_local_testnet_config() {
        let config = ChainConfig::local_testnet();
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_serialize_local_testnet_config() {
        let config = ChainConfig::local_testnet();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig = serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_contract_state() {
        let config = test_config_contract_state();
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_contract_state() {
        let config = test_config_contract_state();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig = serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_coin_state() {
        let config = test_config_coin_state();
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_coin_state() {
        let config = test_config_coin_state();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig = serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    fn test_config_contract_state() -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let test_key: Bytes32 = rng.gen();
        let test_value: Bytes32 = rng.gen();
        let state = vec![(test_key, test_value)];
        let contract = Contract::from(Opcode::RET(0x10).to_bytes().to_vec());

        ChainConfig {
            chain_name: "".to_string(),
            block_production: ProductionStrategy::Instant,
            parent_network: BaseChainConfig::LocalTest,
            initial_state: StateConfig {
                coins: vec![],
                contracts: vec![ContractConfig {
                    code: contract.into(),
                    salt: Default::default(),
                    state: Some(state),
                }],
                height: None,
            },
        }
    }

    fn test_config_coin_state() -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let utxo_id: Option<Bytes32> = Some(rng.gen());
        let block_created = Some(rng.next_u32().into());
        let maturity = Some(rng.next_u32().into());
        let owner = rng.gen();
        let amount = rng.gen();
        let color = rng.gen();

        ChainConfig {
            chain_name: "".to_string(),
            block_production: ProductionStrategy::Instant,
            parent_network: BaseChainConfig::LocalTest,
            initial_state: StateConfig {
                coins: vec![CoinConfig {
                    utxo_id,
                    block_created,
                    maturity,
                    owner,
                    amount,
                    color,
                }],
                contracts: vec![],
                height: None,
            },
        }
    }
}
