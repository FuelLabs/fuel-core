mod chain;
mod coin;
mod consensus;
mod contract;
mod message;
mod state;

pub use chain::*;
pub use coin::*;
pub use consensus::*;
pub use contract::*;
pub use message::*;
pub use state::*;

#[cfg(test)]
mod tests {
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_asm::op,
        fuel_types::{
            AssetId,
            Bytes32,
        },
        fuel_vm::Contract,
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
        path::PathBuf,
    };

    use super::{
        chain::ChainConfig,
        coin::CoinConfig,
        contract::ContractConfig,
        message::MessageConfig,
        state::StateConfig,
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
        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());

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
                    da_height: DaBlockHeight(rng.gen()),
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
