use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        BlockHeight,
        ContractId,
    },
    fuel_vm::SecretKey,
};

#[cfg(feature = "std")]
use bech32::{
    ToBase32,
    Variant::Bech32m,
};
#[cfg(feature = "std")]
use core::str::FromStr;
#[cfg(feature = "std")]
use fuel_core_types::fuel_types::Bytes32;
#[cfg(feature = "std")]
use itertools::Itertools;
use serde::{
    Deserialize,
    Serialize,
};
#[cfg(feature = "std")]
use std::path::Path;

use super::{
    coin::CoinConfig,
    contract::ContractConfig,
    contract_balance::ContractBalance,
    contract_state::ContractStateConfig,
    message::MessageConfig,
};

// Fuel Network human-readable part for bech32 encoding
pub const FUEL_BECH32_HRP: &str = "fuel";
pub const TESTNET_INITIAL_BALANCE: u64 = 10_000_000;

pub const TESTNET_WALLET_SECRETS: [&str; 5] = [
    "0xde97d8624a438121b86a1956544bd72ed68cd69f2c99555b08b1e8c51ffd511c",
    "0x37fa81c84ccd547c30c176b118d5cb892bdb113e8e80141f266519422ef9eefd",
    "0x862512a2363db2b3a375c0d4bbbd27172180d89f23f2e259bac850ab02619301",
    "0x976e5c3fa620092c718d852ca703b6da9e3075b9f2ecb8ed42d9f746bf26aafb",
    "0x7f8a325504e7315eda997db7861c9447f5c3eff26333b20180475d94443a10c6",
];

pub const CHAIN_STATE_FILENAME: &str = "chain_state.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct StateConfig {
    /// Spendable coins
    pub coins: Vec<CoinConfig>,
    /// Messages from Layer 1
    pub messages: Vec<MessageConfig>,
    /// Contract state
    pub contracts: Vec<ContractConfig>,
    /// State entries of all contracts
    pub contract_state: Vec<ContractStateConfig>,
    /// Balance entries of all contracts
    pub contract_balance: Vec<ContractBalance>,
}

impl StateConfig {
    pub fn generate_state_config(db: impl ChainStateDb) -> StorageResult<Self> {
        let coins = db.iter_coin_configs().try_collect()?;
        let messages = db.iter_message_configs().try_collect()?;
        let contracts = db.iter_contract_configs().try_collect()?;
        let contract_state = db.iter_contract_state_configs().try_collect()?;
        let contract_balance = db.iter_contract_balance_configs().try_collect()?;

        Ok(Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
        })
    }

    #[cfg(feature = "std")]
    pub fn load_from_directory(path: impl AsRef<Path>) -> Result<Self, anyhow::Error> {
        use crate::StateStreamer;

        let decoder = StateStreamer::detect_encoding(path, 1)?;

        let coins = decoder
            .coins()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let messages = decoder
            .messages()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contracts = decoder
            .contracts()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contract_state = decoder
            .contract_state()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        let contract_balance = decoder
            .contract_balance()?
            .map_ok(|group| group.data)
            .flatten_ok()
            .try_collect()?;

        Ok(Self {
            coins,
            messages,
            contracts,
            contract_state,
            contract_balance,
        })
    }

    #[cfg(feature = "std")]
    pub fn create_config_file(self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        // TODO add parquet wrtter once fully implemented
        let mut writer = crate::Encoder::json(path);

        writer.write_coins(self.coins)?;
        writer.write_messages(self.messages)?;
        writer.write_contracts(self.contracts)?;
        writer.write_contract_state(self.contract_state)?;
        writer.write_contract_balance(self.contract_balance)?;
        writer.close()?;

        Ok(())
    }

    #[cfg(feature = "std")]
    pub fn local_testnet() -> Self {
        // endow some preset accounts with an initial balance
        tracing::info!("Initial Accounts");
        let coins = TESTNET_WALLET_SECRETS
            .into_iter()
            .map(|secret| {
                let secret = SecretKey::from_str(secret).expect("Expected valid secret");
                let address = Address::from(*secret.public_key().hash());
                let bech32_data = Bytes32::new(*address).to_base32();
                let bech32_encoding =
                    bech32::encode(FUEL_BECH32_HRP, bech32_data, Bech32m).unwrap();
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x} [bech32: {}]), Balance({})",
                    secret,
                    address,
                    bech32_encoding,
                    TESTNET_INITIAL_BALANCE
                );
                Self::initial_coin(secret, TESTNET_INITIAL_BALANCE, None)
            })
            .collect_vec();

        Self {
            coins,
            ..StateConfig::default()
        }
    }

    #[cfg(feature = "random")]
    pub fn random_testnet() -> Self {
        tracing::info!("Initial Accounts");
        let mut rng = rand::thread_rng();
        let coins = (0..5)
            .map(|_| {
                let secret = SecretKey::random(&mut rng);
                let address = Address::from(*secret.public_key().hash());
                let bech32_data = Bytes32::new(*address).to_base32();
                let bech32_encoding =
                    bech32::encode(FUEL_BECH32_HRP, bech32_data, Bech32m).unwrap();
                tracing::info!(
                    "PrivateKey({:#x}), Address({:#x} [bech32: {}]), Balance({})",
                    secret,
                    address,
                    bech32_encoding,
                    TESTNET_INITIAL_BALANCE
                );
                Self::initial_coin(secret, TESTNET_INITIAL_BALANCE, None)
            })
            .collect_vec();

        Self {
            coins,
            ..StateConfig::default()
        }
    }

    pub fn initial_coin(
        secret: SecretKey,
        amount: u64,
        utxo_id: Option<UtxoId>,
    ) -> CoinConfig {
        let address = Address::from(*secret.public_key().hash());

        CoinConfig {
            tx_id: utxo_id.as_ref().map(|u| *u.tx_id()),
            output_index: utxo_id.as_ref().map(|u| u.output_index()),
            tx_pointer_block_height: None,
            tx_pointer_tx_idx: None,
            maturity: None,
            owner: address,
            amount,
            asset_id: Default::default(),
        }
    }
}

pub trait ChainStateDb {
    /// Returns the contract config with the given contract id.
    fn get_contract_config_by_id(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<ContractConfig>;
    /// Returns *all* unspent coin configs available in the database.
    fn iter_coin_configs(&self) -> impl Iterator<Item = StorageResult<CoinConfig>>;
    /// Returns *alive* contract configs available in the database.
    fn iter_contract_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractConfig>>;

    /// Returns the state of all contracts
    fn iter_contract_state_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractStateConfig>>;
    /// Returns the balances of all contracts
    fn iter_contract_balance_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractBalance>>;
    /// Returns *all* unspent message configs available in the database.
    fn iter_message_configs(&self) -> impl Iterator<Item = StorageResult<MessageConfig>>;
    /// Returns the last available block height.
    fn get_block_height(&self) -> StorageResult<BlockHeight>;
}

impl<T> ChainStateDb for &T
where
    T: ChainStateDb,
{
    fn get_contract_config_by_id(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<ContractConfig> {
        (*self).get_contract_config_by_id(contract_id)
    }

    fn iter_coin_configs(&self) -> impl Iterator<Item = StorageResult<CoinConfig>> {
        (*self).iter_coin_configs()
    }

    fn iter_contract_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractConfig>> {
        (*self).iter_contract_configs()
    }

    fn iter_contract_state_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractStateConfig>> {
        (*self).iter_contract_state_configs()
    }

    fn iter_contract_balance_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractBalance>> {
        (*self).iter_contract_balance_configs()
    }

    fn iter_message_configs(&self) -> impl Iterator<Item = StorageResult<MessageConfig>> {
        (*self).iter_message_configs()
    }

    fn get_block_height(&self) -> StorageResult<BlockHeight> {
        (*self).get_block_height()
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        fuel_asm::op,
        fuel_tx::{
            TxPointer,
            UtxoId,
        },
        fuel_types::{
            AssetId,
            Bytes32,
        },
        fuel_vm::Contract,
    };
    use rand::{
        rngs::StdRng,
        Rng,
        RngCore,
        SeedableRng,
    };

    use crate::{
        CoinConfig,
        ContractConfig,
        MessageConfig,
    };

    #[cfg(feature = "std")]
    use std::env::temp_dir;

    use super::StateConfig;

    #[cfg(feature = "std")]
    #[test]
    fn can_roundrip_write_read() {
        let tmp_file = temp_dir();
        let disk_config = StateConfig::local_testnet();

        disk_config.clone().create_config_file(&tmp_file).unwrap();

        let load_config = StateConfig::load_from_directory(&tmp_file).unwrap();

        assert_eq!(disk_config, load_config);
    }

    #[test]
    fn snapshot_simple_contract() {
        let config = test_config_contract(false, false, false, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_simple_contract() {
        let config = test_config_contract(false, false, false, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: StateConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_state() {
        let config = test_config_contract(true, false, false, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_state() {
        let config = test_config_contract(true, false, false, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: StateConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_balances() {
        let config = test_config_contract(false, true, false, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_balances() {
        let config = test_config_contract(false, true, false, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: StateConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_utxo_id() {
        let config = test_config_contract(false, false, true, false);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_utxoid() {
        let config = test_config_contract(false, false, true, false);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: StateConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    #[test]
    fn snapshot_contract_with_tx_pointer() {
        let config = test_config_contract(false, false, false, true);
        let json = serde_json::to_string_pretty(&config).unwrap();
        insta::assert_snapshot!(json);
    }

    #[test]
    fn can_roundtrip_contract_with_tx_pointer() {
        let config = test_config_contract(false, false, false, true);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized_config: StateConfig =
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
        let deserialized_config: StateConfig =
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
        let deserialized_config: StateConfig =
            serde_json::from_str(json.as_str()).unwrap();
        assert_eq!(config, deserialized_config);
    }

    fn test_config_contract(
        state: bool,
        balances: bool,
        utxo_id: bool,
        tx_pointer: bool,
    ) -> StateConfig {
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
        let utxo_id = if utxo_id {
            Some(UtxoId::new(rng.gen(), rng.gen()))
        } else {
            None
        };
        let tx_pointer = if tx_pointer {
            Some(TxPointer::new(rng.gen(), rng.gen()))
        } else {
            None
        };

        let contract = Contract::from(op::ret(0x10).to_bytes().to_vec());

        StateConfig {
            contracts: vec![ContractConfig {
                contract_id: Default::default(),
                code: contract.into(),
                salt: Default::default(),
                state,
                balances,
                tx_id: utxo_id.map(|utxo_id| *utxo_id.tx_id()),
                output_index: utxo_id.map(|utxo_id| utxo_id.output_index()),
                tx_pointer_block_height: tx_pointer.map(|p| p.block_height()),
                tx_pointer_tx_idx: tx_pointer.map(|p| p.tx_index()),
            }],
            ..Default::default()
        }
    }

    fn test_config_coin_state() -> StateConfig {
        let mut rng = StdRng::seed_from_u64(1);
        let tx_id: Option<Bytes32> = Some(rng.gen());
        let output_index: Option<u8> = Some(rng.gen());
        let block_created = Some(rng.next_u32().into());
        let block_created_tx_idx = Some(rng.gen());
        let maturity = Some(rng.next_u32().into());
        let owner = rng.gen();
        let amount = rng.gen();
        let asset_id = rng.gen();

        StateConfig {
            coins: vec![CoinConfig {
                tx_id,
                output_index,
                tx_pointer_block_height: block_created,
                tx_pointer_tx_idx: block_created_tx_idx,
                maturity,
                owner,
                amount,
                asset_id,
            }],
            ..Default::default()
        }
    }

    fn test_message_config() -> StateConfig {
        let mut rng = StdRng::seed_from_u64(1);

        StateConfig {
            messages: vec![MessageConfig {
                sender: rng.gen(),
                recipient: rng.gen(),
                nonce: rng.gen(),
                amount: rng.gen(),
                data: vec![rng.gen()],
                da_height: DaBlockHeight(rng.gen()),
            }],
            ..Default::default()
        }
    }
}
