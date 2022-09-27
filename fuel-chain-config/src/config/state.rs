use crate::serialization::HexNumber;

use fuel_core_interfaces::{
    db::Error,
    model::BlockHeight,
};

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
    pub fn generate_state_config<T>(db: T) -> anyhow::Result<Self>
    where
        T: ChainConfigDb,
    {
        Ok(StateConfig {
            coins: db.get_coin_config()?,
            contracts: db.get_contract_config()?,
            messages: db.get_message_config()?,
            height: db.get_block_height()?,
        })
    }
}

pub trait ChainConfigDb {
    fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>>;
    fn get_contract_config(&self) -> Result<Option<Vec<ContractConfig>>, anyhow::Error>;
    fn get_message_config(&self) -> Result<Option<Vec<MessageConfig>>, Error>;
    fn get_block_height(&self) -> Result<Option<BlockHeight>, Error>;
}
