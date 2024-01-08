use bech32::{
    ToBase32,
    Variant::Bech32m,
};
use core::str::FromStr;
use fuel_core_storage::{
    iter::BoxedIter,
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        BlockHeight,
        ContractId,
    },
    fuel_vm::SecretKey,
};
use itertools::Itertools;

use fuel_core_types::fuel_types::Bytes32;
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

pub const STATE_CONFIG_FILENAME: &str = "state_config.json";

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
    pub fn load_from_directory(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        use crate::Decoder;

        let decoder = Decoder::detect_encoding(path, 1)?;

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

// TODO: BoxedIter to be used until RPITIT lands in stable rust.
#[impl_tools::autoimpl(for<T: trait> &T, &mut T)]
pub trait ChainStateDb {
    /// Returns the contract config with the given contract id.
    fn get_contract_config_by_id(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<ContractConfig>;
    /// Returns *all* unspent coin configs available in the database.
    fn iter_coin_configs(&self) -> BoxedIter<StorageResult<CoinConfig>>;
    /// Returns *alive* contract configs available in the database.
    fn iter_contract_configs(&self) -> BoxedIter<StorageResult<ContractConfig>>;

    /// Returns the state of all contracts
    fn iter_contract_state_configs(
        &self,
    ) -> BoxedIter<StorageResult<ContractStateConfig>>;
    /// Returns the balances of all contracts
    fn iter_contract_balance_configs(&self) -> BoxedIter<StorageResult<ContractBalance>>;
    /// Returns *all* unspent message configs available in the database.
    fn iter_message_configs(&self) -> BoxedIter<StorageResult<MessageConfig>>;
    /// Returns the last available block height.
    fn get_block_height(&self) -> StorageResult<BlockHeight>;
}
