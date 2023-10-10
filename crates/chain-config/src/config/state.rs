use crate::{
    FUEL_BECH32_HRP,
    TESTNET_INITIAL_BALANCE,
};
use bech32::{
    ToBase32,
    Variant::Bech32m,
};
use fuel_core_types::{
    fuel_tx::UtxoId,
    fuel_types::{
        Address,
        BlockHeight,
        Bytes32,
    },
    fuel_vm::SecretKey,
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

use crate::config::coin::CoinConfig;

use fuel_core_storage::Result as StorageResult;

use super::{
    contract::ContractConfig,
    message::MessageConfig,
};

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
}

impl StateConfig {
    pub fn generate_state_config<T>(db: T) -> StorageResult<Self>
    where
        T: ChainConfigDb,
    {
        Ok(StateConfig {
            coins: db.get_coin_config()?,
            contracts: db.get_contract_config()?,
            messages: db.get_message_config()?,
        })
    }

    pub fn local_testnet() -> Self {
        // endow some preset accounts with an initial balance
        tracing::info!("Initial Accounts");
        let mut rng = StdRng::seed_from_u64(10);
        let _initial_coins = (0..5)
            .map(|_| {
                let secret = fuel_core_types::fuel_crypto::SecretKey::random(&mut rng);
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
            coins: Some(_initial_coins),
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

pub trait ChainConfigDb {
    /// Returns *all* unspent coin configs available in the database.
    fn get_coin_config(&self) -> StorageResult<Option<Vec<CoinConfig>>>;
    /// Returns *alive* contract configs available in the database.
    fn get_contract_config(&self) -> StorageResult<Option<Vec<ContractConfig>>>;
    /// Returns *all* unspent message configs available in the database.
    fn get_message_config(&self) -> StorageResult<Option<Vec<MessageConfig>>>;
    /// Returns the last available block height.
    fn get_block_height(&self) -> StorageResult<BlockHeight>;
}
