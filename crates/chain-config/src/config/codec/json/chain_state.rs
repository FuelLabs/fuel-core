use crate::{
    config::{contract_balance::ContractBalance, contract_state::ContractState},
    CoinConfig, ContractConfig, MessageConfig,
};
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainState {
    pub(crate) coins: Vec<CoinConfig>,
    pub(crate) messages: Vec<MessageConfig>,
    pub(crate) contracts: Vec<ContractConfig>,
    pub(crate) contract_state: Vec<ContractState>,
    pub(crate) contract_balance: Vec<ContractBalance>,
}

#[cfg(all(test, feature = "random"))]
impl ChainState {
    pub fn random(amount: usize, per_contract: usize, rng: &mut impl rand::Rng) -> Self {
        Self {
            coins: std::iter::repeat_with(|| CoinConfig::random(rng))
                .take(amount)
                .collect(),
            messages: std::iter::repeat_with(|| MessageConfig::random(rng))
                .take(amount)
                .collect(),
            contracts: std::iter::repeat_with(|| ContractConfig::random(rng))
                .take(amount)
                .collect(),
            contract_state: std::iter::repeat_with(|| ContractState::random(rng))
                .take(amount)
                .collect(),
            contract_balance: std::iter::repeat_with(|| ContractBalance::random(rng))
                .take(amount)
                .collect(),
        }
    }
}
