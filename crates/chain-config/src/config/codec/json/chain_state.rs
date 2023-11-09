use itertools::Itertools;

use crate::{
    config::{contract_balance::ContractBalance, contract_state::ContractState},
    CoinConfig, ContractConfig, MessageConfig,
};

pub(crate) struct JsonBatchReader {
    source: ChainState,
    batch_size: usize,
    current_batch_cursor: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChainState {
    pub(crate) coins: Vec<CoinConfig>,
    pub(crate) messages: Vec<MessageConfig>,
    pub(crate) contracts: Vec<ContractConfig>,
    pub(crate) contract_state: Vec<Vec<ContractState>>,
    pub(crate) contract_balance: Vec<Vec<ContractBalance>>,
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
                .chunks(100)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(amount)
                .collect(),
            contract_balance: std::iter::repeat_with(|| ContractBalance::random(rng))
                .chunks(100)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(amount)
                .collect(),
        }
    }
}
