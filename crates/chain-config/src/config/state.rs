use crate::serialization::HexNumber;

use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_types::BlockHeight;

use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

use super::{
    coin::CoinConfig,
    contract::ContractConfig,
    message::MessageConfig,
};

// TODO: do streaming deserialization to handle large state configs
#[serde_as]
#[skip_serializing_none]
#[derive(Default, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
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
    pub fn generate_state_config<T>(db: T) -> StorageResult<Self>
    where
        T: ChainConfigDb,
    {
        Ok(StateConfig {
            coins: db.get_coin_config()?,
            contracts: db.get_contract_config()?,
            messages: db.get_message_config()?,
            height: Some(db.get_block_height()?),
        })
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
