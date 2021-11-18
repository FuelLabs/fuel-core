use self::serialization::HexType;
use crate::model::fuel_block::BlockHeight;
use fuel_types::{Address, Bytes32, Color, Salt};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use std::{io::ErrorKind, path::PathBuf, str::FromStr};

pub mod serialization;

pub const LOCAL_TESTNET: &'static str = "local_testnet";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ChainConfig {
    pub chain_name: String,
    pub block_production: ProductionStrategy,
    pub parent_network: BaseChainConfig,
    pub initial_state: Option<StateConfig>,
}

impl ChainConfig {
    pub fn local_testnet() -> Self {
        // endow 10 mock accounts with an initial balance
        let initial_coins = (1..10)
            .map(|idx| CoinConfig {
                utxo_id: None,
                block_created: None,
                maturity: None,
                owner: Address::new([idx; 32]),
                amount: TESTNET_INITIAL_BALANCE,
                color: Default::default(),
            })
            .collect_vec();

        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            block_production: ProductionStrategy::Instant,
            parent_network: BaseChainConfig::LocalTest,
            initial_state: Some(StateConfig {
                coins: Some(initial_coins),
                ..StateConfig::default()
            }),
        }
    }
}

impl FromStr for ChainConfig {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            LOCAL_TESTNET => Ok(Self::local_testnet()),
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
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Option<Vec<CoinConfig>>,
    /// Contract state
    pub contracts: Option<Vec<ContractConfig>>,
    /// Starting block height (useful for flattened fork networks)
    pub height: Option<BlockHeight>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    #[serde(default)]
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

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ContractConfig {
    #[serde_as(as = "HexType")]
    pub code: Vec<u8>,
    #[serde_as(as = "HexType")]
    pub salt: Salt,
    #[serde_as(as = "Option<Vec<(HexType, HexType)>>")]
    #[serde(default)]
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
    fn snapshot_configurable_block_height() {
        let mut rng = StdRng::seed_from_u64(2);
        let config = ChainConfig {
            initial_state: Some(StateConfig {
                height: Some(rng.next_u32().into()),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        };
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_serialize_block_height_config() {
        let mut rng = StdRng::seed_from_u64(2);
        let config = ChainConfig {
            initial_state: Some(StateConfig {
                height: Some(rng.next_u32().into()),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig = serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_contract() {
        let config = test_config_contract(false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_contract() {
        let config = test_config_contract(false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig = serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_contract_with_state() {
        let config = test_config_contract(true);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_contract_with_state() {
        let config = test_config_contract(true);
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

    fn test_config_contract(state: bool) -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let state = if state {
            let test_key: Bytes32 = rng.gen();
            let test_value: Bytes32 = rng.gen();
            Some(vec![(test_key, test_value)])
        } else {
            None
        };
        let contract = Contract::from(Opcode::RET(0x10).to_bytes().to_vec());

        ChainConfig {
            initial_state: Some(StateConfig {
                contracts: Some(vec![ContractConfig {
                    code: contract.into(),
                    salt: Default::default(),
                    state,
                }]),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
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
            initial_state: Some(StateConfig {
                coins: Some(vec![CoinConfig {
                    utxo_id,
                    block_created,
                    maturity,
                    owner,
                    amount,
                    color,
                }]),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        }
    }
}
