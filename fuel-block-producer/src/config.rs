use fuel_core_interfaces::common::{
    fuel_tx::ConsensusParameters,
    fuel_types::Word,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub max_gas_per_block: Word,
    pub consensus_params: ConsensusParameters,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_gas_per_block: 1_000_000, // TODO: pick a reasonable default value
            consensus_params: Default::default(),
        }
    }
}
