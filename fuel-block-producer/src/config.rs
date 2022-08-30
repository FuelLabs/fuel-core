use fuel_core_interfaces::common::{
    fuel_tx::ConsensusParameters,
    fuel_types::{Address, Word},
};

#[derive(Clone, Debug)]
pub struct Config {
    /// The primary identity associated with the validator (i.e. staking key)
    pub validator_id: Address,
    pub max_gas_per_block: Word,
    pub consensus_params: ConsensusParameters,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_gas_per_block: 1_000_000, // TODO: pick a reasonable default value
            validator_id: Default::default(),
            consensus_params: Default::default(),
        }
    }
}
