use crate::{
    config::{contract_balance::ContractBalance, contract_state::ContractState},
    CoinConfig, ContractConfig, MessageConfig,
};
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainState {
    pub(crate) coins: Vec<CoinConfig>,
    pub(crate) messages: Vec<MessageConfig>,
    pub(crate) contracts: Vec<ContractConfig>,
    pub(crate) contract_state: Vec<ContractState>,
    pub(crate) contract_balance: Vec<ContractBalance>,
}
