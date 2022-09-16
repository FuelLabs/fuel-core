use crate::{
    database::Database,
    model::BlockHeight,
};
use fuel_core_interfaces::{
    common::{
        fuel_tx::ConsensusParameters,
        fuel_types::{
            Address,
            AssetId,
            Bytes32,
            Salt,
        },
        fuel_vm::fuel_types::Word,
    },
    model::{
        DaBlockHeight,
        Message,
    },
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};
use serialization::{
    HexNumber,
    HexType,
};
use std::{
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
};

pub mod serialization;

pub const LOCAL_TESTNET: &str = "local_testnet";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ChainConfig {
    pub chain_name: String,
    pub block_production: ProductionStrategy,
    #[serde(default)]
    pub initial_state: Option<StateConfig>,
    pub transaction_parameters: ConsensusParameters,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            chain_name: "local".into(),
            block_production: ProductionStrategy::Instant,
            transaction_parameters: ConsensusParameters::DEFAULT,
            initial_state: None,
        }
    }
}

impl ChainConfig {
    pub const BASE_ASSET: AssetId = AssetId::zeroed();

    pub fn local_testnet() -> Self {
        // endow some preset accounts with an initial balance
        tracing::info!("Initial Accounts");
        let mut rng = StdRng::seed_from_u64(10);
        let initial_coins = (0..5)
            .map(|_| {
                let secret = fuel_core_interfaces::common::fuel_crypto::SecretKey::random(
                    &mut rng,
                );
                let address = Address::from(*secret.public_key().hash());
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x}), Balance({})",
                    secret,
                    address,
                    TESTNET_INITIAL_BALANCE
                );
                CoinConfig {
                    tx_id: None,
                    output_index: None,
                    block_created: None,
                    maturity: None,
                    owner: address,
                    amount: TESTNET_INITIAL_BALANCE,
                    asset_id: Default::default(),
                }
            })
            .collect_vec();

        Self {
            chain_name: LOCAL_TESTNET.to_string(),
            block_production: ProductionStrategy::Instant,
            initial_state: Some(StateConfig {
                coins: Some(initial_coins),
                ..StateConfig::default()
            }),
            transaction_parameters: ConsensusParameters::DEFAULT,
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
                serde_json::from_slice(&contents).map_err(|e| {
                    std::io::Error::new(
                        ErrorKind::InvalidData,
                        anyhow::Error::new(e).context(format!(
                            "an error occurred while loading the chain config file {}",
                            s
                        )),
                    )
                })
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub enum ProductionStrategy {
    Instant,
    Manual,
    RoundRobin,
    ProofOfStake,
}

// TODO: do streaming deserialization to handle large state configs
#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Option<Vec<CoinConfig>>,
    /// Contract state
    pub contracts: Option<Vec<ContractConfig>>,
    /// Messages from Layer 1
    pub messages: Option<Vec<MessageConfig>>,
    /// Starting block height (useful for flattened fork networks)
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub height: Option<BlockHeight>,
}

impl StateConfig {
    pub fn generate_state_config(db: Database) -> anyhow::Result<Self> {
        Ok(StateConfig {
            coins: db.get_coin_config()?,
            contracts: db.get_contract_config()?,
            messages: db.get_message_config()?,
            height: db.get_block_height()?,
        })
    }
}

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct CoinConfig {
    /// auto-generated if None
    #[serde_as(as = "Option<HexType>")]
    #[serde(default)]
    pub tx_id: Option<Bytes32>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub output_index: Option<u64>,
    /// used if coin is forked from another chain to preserve id
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub block_created: Option<BlockHeight>,
    #[serde_as(as = "Option<HexNumber>")]
    #[serde(default)]
    pub maturity: Option<BlockHeight>,
    #[serde_as(as = "HexType")]
    pub owner: Address,
    #[serde_as(as = "HexNumber")]
    pub amount: u64,
    #[serde_as(as = "HexType")]
    pub asset_id: AssetId,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContractConfig {
    #[serde_as(as = "HexType")]
    pub code: Vec<u8>,
    #[serde_as(as = "HexType")]
    pub salt: Salt,
    #[serde_as(as = "Option<Vec<(HexType, HexType)>>")]
    #[serde(default)]
    pub state: Option<Vec<(Bytes32, Bytes32)>>,
    #[serde_as(as = "Option<Vec<(HexType, HexNumber)>>")]
    #[serde(default)]
    pub balances: Option<Vec<(AssetId, u64)>>,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    #[serde_as(as = "HexType")]
    pub sender: Address,
    #[serde_as(as = "HexType")]
    pub recipient: Address,
    #[serde_as(as = "HexNumber")]
    pub nonce: Word,
    #[serde_as(as = "HexNumber")]
    pub amount: Word,
    #[serde_as(as = "HexType")]
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    #[serde_as(as = "HexNumber")]
    pub da_height: DaBlockHeight,
}

impl From<MessageConfig> for Message {
    fn from(msg: MessageConfig) -> Self {
        Message {
            sender: msg.sender,
            recipient: msg.recipient,
            nonce: msg.nonce,
            amount: msg.amount,
            data: msg.data,
            da_height: msg.da_height,
            fuel_block_spend: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::{
        fuel_asm::Opcode,
        fuel_vm::prelude::Contract,
    };
    use rand::{
        prelude::StdRng,
        Rng,
        RngCore,
        SeedableRng,
    };
    use std::{
        env::temp_dir,
        fs::write,
    };

    #[test]
    fn from_str_loads_from_file() {
        // setup chain config in a temp file
        let tmp_file = tmp_path();
        let disk_config = ChainConfig::local_testnet();
        let json = serde_json::to_string_pretty(&disk_config).unwrap();
        write(tmp_file.clone(), json).unwrap();

        // test loading config from file path string
        let load_config: ChainConfig =
            tmp_file.to_string_lossy().into_owned().parse().unwrap();
        assert_eq!(disk_config, load_config);
    }

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
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
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
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_contract() {
        let config = test_config_contract(false, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_contract() {
        let config = test_config_contract(false, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_state() {
        let config = test_config_contract(true, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_state() {
        let config = test_config_contract(true, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_balances() {
        let config = test_config_contract(false, true);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_balances() {
        let config = test_config_contract(false, true);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
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
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_simple_message_state() {
        let config = test_message_config();
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_message_state() {
        let config = test_message_config();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: ChainConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    fn test_config_contract(state: bool, balances: bool) -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let state = if state {
            let test_key: Bytes32 = rng.gen();
            let test_value: Bytes32 = rng.gen();
            Some(vec![(test_key, test_value)])
        } else {
            None
        };
        let balances = if balances {
            let test_asset_id: AssetId = rng.gen();
            let test_balance: u64 = rng.next_u64();
            Some(vec![(test_asset_id, test_balance)])
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
                    balances,
                }]),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        }
    }

    fn test_config_coin_state() -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let tx_id: Option<Bytes32> = Some(rng.gen());
        let output_index: Option<u64> = Some(rng.gen());
        let block_created = Some(rng.next_u32().into());
        let maturity = Some(rng.next_u32().into());
        let owner = rng.gen();
        let amount = rng.gen();
        let asset_id = rng.gen();

        ChainConfig {
            initial_state: Some(StateConfig {
                coins: Some(vec![CoinConfig {
                    tx_id,
                    output_index,
                    block_created,
                    maturity,
                    owner,
                    amount,
                    asset_id,
                }]),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        }
    }

    fn test_message_config() -> ChainConfig {
        let mut rng = StdRng::seed_from_u64(1);

        ChainConfig {
            initial_state: Some(StateConfig {
                messages: Some(vec![MessageConfig {
                    sender: rng.gen(),
                    recipient: rng.gen(),
                    nonce: rng.gen(),
                    amount: rng.gen(),
                    data: vec![rng.gen()],
                    da_height: rng.gen(),
                }]),
                ..Default::default()
            }),
            ..ChainConfig::local_testnet()
        }
    }

    fn tmp_path() -> PathBuf {
        let mut path = temp_dir();
        path.push(rand::random::<u16>().to_string());
        path
    }
}
